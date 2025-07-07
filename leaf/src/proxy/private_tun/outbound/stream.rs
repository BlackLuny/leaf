use std::{io, sync::Arc, time::Duration};

use async_ringbuf::traits::AsyncProducer;
use async_trait::async_trait;
use private_tun::snell_impl_ver::{client_zfc::ConnType, udp_intf::AsyncRingBufSender};
use tokio::io::duplex;
use tokio::sync::oneshot;
use tracing::debug;

use crate::{
    common, option,
    proxy::{AnyStream, Color, OutboundConnect, OutboundStreamHandler, Tag, TcpConnector},
    session::{Network, Session},
};

use super::Client;

pub struct Handler {
    pub tag: String,
    pub color: colored::Color,
    pub client: Arc<Client>,
}

impl TcpConnector for Handler {}

impl Tag for Handler {
    fn tag(&self) -> &String {
        &self.tag
    }
}

impl Color for Handler {
    fn color(&self) -> &colored::Color {
        &self.color
    }
}

#[async_trait]
impl OutboundStreamHandler for Handler {
    async fn connect_addr(&self, sess: &Session) -> OutboundConnect {
        // let (rst_tx, rst_rx) = oneshot::channel();
        // let target = match &sess.destination {
        //     crate::session::SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
        //     crate::session::SocksAddr::Domain(domain, port) => {
        //         private_tun::address::Address::Domain(domain.clone().into(), *port)
        //     }
        // };
        // let server = self
        //     .client
        //     .push_event(ConnType::GetCurrentServer { rst_tx, target })
        //     .await;
        // let (_, server_addr) = rst_rx.await.unwrap();
        // match server_addr {
        //     private_tun::address::Address::Socket(addr) => {
        //         let port = addr.port();
        //         OutboundConnect::Proxy(Network::Tcp, addr.to_string(), port)
        //     }
        //     private_tun::address::Address::Domain(domain, port) => {
        //         OutboundConnect::Proxy(Network::Tcp, domain.to_string(), port)
        //     }
        // }
        OutboundConnect::Unknown
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        _lhs: Option<&mut AnyStream>,
        stream: Option<AnyStream>,
    ) -> io::Result<AnyStream> {
        // let mut stream =
        //     stream.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing stream"))?;

        // Create a duplex channel to bridge leaf's stream with private_tun
        let (client_side_pipe0, client_side_pipe1) = duplex(8192);
        // let (server_side_pipe0, mut server_side_pipe1) = duplex(8192);

        // Create MemDuplex using private_tun's implementation
        let mem_duplex = private_tun::self_proxy::MemDuplex::new(
            client_side_pipe0,
            self.client.cancel_token.clone(),
        );

        // let server_mem_duplex = private_tun::self_proxy::MemDuplex::new(
        //     server_side_pipe0,
        //     self.client.cancel_token.clone(),
        // );

        // Convert destination to private_tun's Address format
        let target = match &sess.destination {
            crate::session::SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            crate::session::SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        // Create oneshot channel for response
        let (rst_tx, _rst_rx) = oneshot::channel();
        // let cancel_token = self.client.cancel_token.clone();
        // // copy data between stream(vpn outbound stream) and server_side_pipe1 (processed by private_tun)
        // tokio::spawn(async move {
        //     tokio::select! {
        //         biased;
        //         r = common::io::copy_buf_bidirectional_with_timeout(
        //             &mut stream,
        //             &mut server_side_pipe1,
        //             *option::LINK_BUFFER_SIZE * 1024,
        //             Duration::from_secs(*option::TCP_UPLINK_TIMEOUT),
        //             Duration::from_secs(*option::TCP_DOWNLINK_TIMEOUT),
        //         ) => {
        //         match r {
        //             Ok((up_count, down_count)) => {}
        //             Err(e) => {}
        //         }
        //         }
        //         _ = cancel_token.cancelled() => {
        //             return;
        //         }
        //     }
        // });

        // Create ConnType for Duplex connection
        let conn_type = private_tun::snell_impl_ver::client_zfc::ConnType::Duplex {
            stream: mem_duplex,
            target,
            rst_tx,
            server_name: None,
            traffic_collector: None,
            one_rtt: false,
            reuse_tcp: true,
            piped_stream: None,
        };

        // Send connection request to private_tun client
        self.client.push_event(conn_type).await.map_err(|_e| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("failed to send connection request"),
            )
        })?;

        Ok(Box::new(client_side_pipe1))
    }
}
