use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures::{sink::SinkExt, stream::StreamExt};
use protobuf::Message;
use tokio::sync::mpsc::channel as tokio_channel;
use tokio::sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender};
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};
use tun::AbstractDevice;

use crate::proxy::tun::TunInfo;
use crate::{
    app::dispatcher::Dispatcher,
    app::fake_dns::{FakeDns, FakeDnsMode},
    app::nat_manager::NatManager,
    app::nat_manager::UdpPacket,
    config::{Inbound, TunInboundSettings},
    option,
    session::{DatagramSource, Network, Session, SocksAddr},
    Runner,
};

use super::netstack;

async fn handle_inbound_stream(
    stream: netstack::TcpStream,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    inbound_tag: String,
    dispatcher: Arc<Dispatcher>,
    fakedns: Arc<FakeDns>,
) {
    let mut sess = Session {
        network: Network::Tcp,
        source: local_addr,
        local_addr: remote_addr,
        destination: SocksAddr::Ip(remote_addr),
        inbound_tag,
        ..Default::default()
    };
    // Whether to override the destination according to Fake DNS.
    if fakedns.is_fake_ip(&remote_addr.ip()).await {
        if let Some(domain) = fakedns.query_domain(&remote_addr.ip()).await {
            sess.destination = SocksAddr::Domain(domain, remote_addr.port());
        } else {
            // Although requests targeting fake IPs are assumed
            // never happen in real network traffic, which are
            // likely caused by poisoned DNS cache records, we
            // still have a chance to sniff the request domain
            // for TLS traffic in dispatcher.
            if remote_addr.port() != 443 {
                debug!(
                    "No paired domain found for this fake IP: {}, connection is rejected.",
                    &remote_addr.ip()
                );
                return;
            }
        }
    }
    dispatcher.dispatch_stream(sess, stream).await;
}

async fn handle_inbound_datagram(
    socket: netstack::UdpSocket,
    inbound_tag: String,
    nat_manager: Arc<NatManager>,
    fakedns: Arc<FakeDns>,
) {
    // The socket to receive/send packets from/to the netstack.
    let (mut lr, ls) = socket.split();
    let ls = Arc::new(Mutex::new(ls));

    // The channel for sending back datagrams from NAT manager to netstack.
    let (l_tx, mut l_rx): (TokioSender<UdpPacket>, TokioReceiver<UdpPacket>) =
        tokio_channel(*crate::option::UDP_DOWNLINK_CHANNEL_SIZE);

    // Receive datagrams from NAT manager and send back to netstack.
    let fakedns_cloned = fakedns.clone();
    let ls_cloned = ls.clone();
    tokio::spawn(async move {
        while let Some(pkt) = l_rx.recv().await {
            let src_addr = match pkt.src_addr {
                SocksAddr::Ip(a) => a,
                SocksAddr::Domain(domain, port) => {
                    if let Some(ip) = fakedns_cloned.query_fake_ip(&domain).await {
                        SocketAddr::new(ip, port)
                    } else {
                        warn!(
                                "Received datagram with source address {}:{} without paired fake IP found.",
                                &domain, &port
                            );
                        continue;
                    }
                }
            };
            let mut ls_locked = ls_cloned.lock().await;
            if let Err(e) = ls_locked
                .send((
                    pkt.data[..].to_vec(),
                    src_addr,
                    pkt.dst_addr.must_ip().clone(),
                ))
                .await
            {
                warn!("A packet failed to send to the netstack: {}", e);
            }
        }
    });

    // Accept datagrams from netstack and send to NAT manager.
    loop {
        match lr.next().await {
            None => {
                warn!("Failed to accept a datagram from netstack");
            }
            Some((data, src_addr, dst_addr)) => {
                // Fake DNS logic.
                if dst_addr.port() == 53 {
                    match fakedns.generate_fake_response(&data).await {
                        Ok(resp) => {
                            let mut ls_locked = ls.lock().await;
                            if let Err(e) = ls_locked.send((resp, dst_addr, src_addr)).await {
                                warn!("A packet failed to send to the netstack: {}", e);
                            }
                            continue;
                        }
                        Err(err) => {
                            trace!("generate fake ip failed: {}", err);
                        }
                    }
                }

                // Whether to override the destination according to Fake DNS.
                //
                // WARNING
                //
                // This allows datagram to have a domain name as destination,
                // but real UDP traffic are sent with IP address only. If the
                // outbound for this datagram is a direct one, the outbound
                // would resolve the domain to IP address before sending out
                // the datagram. If the outbound is a proxy one, it would
                // require a proxy server with the ability to handle datagrams
                // with domain name destination, leaf itself of course supports
                // this feature very well.
                let dst_addr = if fakedns.is_fake_ip(&dst_addr.ip()).await {
                    if let Some(domain) = fakedns.query_domain(&dst_addr.ip()).await {
                        SocksAddr::Domain(domain, dst_addr.port())
                    } else {
                        debug!(
                            "No paired domain found for this fake IP: {}, datagram is rejected.",
                            &dst_addr.ip()
                        );
                        continue;
                    }
                } else {
                    SocksAddr::Ip(dst_addr)
                };

                let dgram_src = DatagramSource::new(src_addr, None);
                let pkt = UdpPacket::new(data, SocksAddr::Ip(src_addr), dst_addr);
                nat_manager
                    .send(None, &dgram_src, &inbound_tag, &l_tx, pkt)
                    .await;
            }
        }
    }
}

pub fn new(
    inbound: Inbound,
    dispatcher: Arc<Dispatcher>,
    nat_manager: Arc<NatManager>,
) -> Result<(Runner, TunInfo)> {
    let settings = TunInboundSettings::parse_from_bytes(&inbound.settings)?;

    let mut cfg = tun::Configuration::default();
    let mut tun_info = TunInfo::default();
    if settings.fd >= 0 {
        cfg.raw_fd(settings.fd);
    } else if settings.auto {
        cfg.tun_name(&*option::DEFAULT_TUN_NAME)
            .address(&*option::DEFAULT_TUN_IPV4_ADDR)
            .destination(&*option::DEFAULT_TUN_IPV4_GW)
            .mtu(1500);

        #[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
        {
            cfg.netmask(&*option::DEFAULT_TUN_IPV4_MASK);
        }
        tun_info.tun_name = option::DEFAULT_TUN_NAME.clone();
        tun_info.tun_ip = option::DEFAULT_TUN_IPV4_ADDR.parse::<IpAddr>().unwrap();
        tun_info.tun_netmask = option::DEFAULT_TUN_IPV4_MASK.parse::<IpAddr>().unwrap();
        tun_info.tun_gateway = option::DEFAULT_TUN_IPV4_GW.parse::<IpAddr>().unwrap();
        tun_info.tun_mtu = 1500;
        cfg.up();
    } else {
        cfg.tun_name(settings.name.clone())
            .address(settings.address.clone())
            .destination(settings.gateway.clone())
            .mtu(settings.mtu as u16);
        tun_info.tun_name = settings.name;
        tun_info.tun_ip = settings.address.parse::<IpAddr>().unwrap();
        tun_info.tun_netmask = settings.netmask.parse::<IpAddr>().unwrap();
        tun_info.tun_gateway = settings.gateway.parse::<IpAddr>().unwrap();
        tun_info.tun_mtu = settings.mtu as u16;

        #[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
        {
            cfg.netmask(settings.netmask);
        }

        cfg.up();
    }

    // FIXME it's a bad design to have 2 lists in config while we need only one
    let fake_dns_exclude = settings.fake_dns_exclude;
    let fake_dns_include = settings.fake_dns_include;
    if !fake_dns_exclude.is_empty() && !fake_dns_include.is_empty() {
        return Err(anyhow!(
            "fake DNS run in either include mode or exclude mode"
        ));
    }
    let (fake_dns_mode, fake_dns_filters) = if !fake_dns_include.is_empty() {
        (FakeDnsMode::Include, fake_dns_include)
    } else {
        (FakeDnsMode::Exclude, fake_dns_exclude)
    };

    let tun = tun::create_as_async(&cfg).map_err(|e| anyhow!("create tun failed: {}", e))?;
    #[cfg(target_os = "macos")]
    {
        tun_info.tun_name = tun.tun_name()?;
    }
    if settings.auto {
        assert!(settings.fd == -1, "tun-auto is not compatible with tun-fd");
    }

    let (stack, runner, udp_socket, tcp_listener) = netstack::StackBuilder::default()
        .enable_tcp(true)
        .enable_udp(true)
        .stack_buffer_size(*crate::option::NETSTACK_OUTPUT_CHANNEL_SIZE)
        .udp_buffer_size(*crate::option::NETSTACK_UDP_UPLINK_CHANNEL_SIZE)
        .tcp_buffer_size(*crate::option::NETSTACK_TCP_UPLINK_CHANNEL_SIZE)
        .build()?;

    Ok((
        Box::pin(async move {
            let fakedns = Arc::new(FakeDns::new(fake_dns_mode));
            for filter in fake_dns_filters.into_iter() {
                fakedns.add_filter(filter).await;
            }

            let inbound_tag = inbound.tag.clone();
            let framed = tun.into_framed();
            let (mut tun_sink, mut tun_stream) = framed.split();
            let (mut stack_sink, mut stack_stream) = stack.split();

            let mut futs: Vec<Runner> = Vec::new();

            futs.push(Box::pin(async move {
                runner.unwrap().await;
            }));

            // Reads packet from stack and sends to TUN.
            futs.push(Box::pin(async move {
                while let Some(pkt) = stack_stream.next().await {
                    match pkt {
                        Ok(pkt) => {
                            if let Err(e) = tun_sink.send(pkt).await {
                                // TODO Return the error
                                error!("Sending packet to TUN failed: {}", e);
                                return;
                            }
                        }
                        Err(e) => {
                            error!("Net stack erorr: {}", e);
                            return;
                        }
                    }
                }
            }));

            // Reads packet from TUN and sends to stack.
            futs.push(Box::pin(async move {
                while let Some(pkt) = tun_stream.next().await {
                    match pkt {
                        Ok(pkt) => {
                            if let Err(e) = stack_sink.send(pkt).await {
                                error!("Sending packet to NetStack failed: {}", e);
                                return;
                            }
                        }
                        Err(e) => {
                            error!("TUN error: {}", e);
                            return;
                        }
                    }
                }
            }));

            // Extracts TCP connections from stack and sends them to the dispatcher.
            let inbound_tag_cloned = inbound_tag.clone();
            let fakedns_cloned = fakedns.clone();
            futs.push(Box::pin(async move {
                let mut tcp_listener = tcp_listener.unwrap();
                while let Some((stream, local_addr, remote_addr)) = tcp_listener.next().await {
                    tokio::spawn(handle_inbound_stream(
                        stream,
                        local_addr,
                        remote_addr,
                        inbound_tag_cloned.clone(),
                        dispatcher.clone(),
                        fakedns_cloned.clone(),
                    ));
                }
            }));

            // Receive and send UDP packets between netstack and NAT manager. The NAT
            // manager would maintain UDP sessions and send them to the dispatcher.
            futs.push(Box::pin(async move {
                handle_inbound_datagram(
                    udp_socket.unwrap(),
                    inbound_tag,
                    nat_manager,
                    fakedns.clone(),
                )
                .await;
            }));

            info!("start tun inbound");
            futures::future::select_all(futs).await;
        }),
        tun_info,
    ))
}
