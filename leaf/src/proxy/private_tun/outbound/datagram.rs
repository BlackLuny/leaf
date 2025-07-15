use std::{io, net::SocketAddr, sync::Arc};

use async_ringbuf::traits::{AsyncConsumer, AsyncProducer};
use async_trait::async_trait;
use private_tun::snell_impl_ver::{
    client_zfc::ConnType,
    udp_intf::{
        create_udp_ringbuf_channel_l2t, create_udp_ringbuf_channel_t2l, AsyncRingBufSender,
        UdpRingBufL2TSender,
    },
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::Client;
use crate::{
    proxy::{
        AnyOutboundDatagram, AnyOutboundTransport, Color, DatagramTransportType, OutboundConnect,
        OutboundDatagram, OutboundDatagramHandler, OutboundDatagramRecvHalf,
        OutboundDatagramSendHalf, OutboundTransport, Tag,
    },
    session::{Network, Session, SocksAddr},
};

pub struct Handler {
    pub tag: String,
    pub color: colored::Color,
    pub bind_addr: SocketAddr,
    pub client: Arc<Client>,
}

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

impl Drop for Handler {
    fn drop(&mut self) {
        self.client.cancel_token.cancel();
    }
}

#[async_trait]
impl OutboundDatagramHandler for Handler {
    async fn connect_addr(&self, _sess: &Session) -> OutboundConnect {
        OutboundConnect::Unknown
    }

    fn transport_type(&self) -> DatagramTransportType {
        DatagramTransportType::Unreliable
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        transport: Option<AnyOutboundTransport>,
    ) -> io::Result<AnyOutboundDatagram> {
        // Convert destination to private_tun's Address format
        let target = match &sess.destination {
            SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        // Create UDP ring buffer channels
        let (l2t_sender, l2t_receiver) = create_udp_ringbuf_channel_l2t();
        let (t2l_sender, t2l_receiver) = create_udp_ringbuf_channel_t2l();

        // Create oneshot channel for response
        let (rst_tx, _rst_rx) = oneshot::channel();

        // Use a default peer address since we don't have socket info
        let peer_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        // Create ConnType for UDP connection
        let conn_type = ConnType::Udp {
            peer_addr,
            target,
            rst_tx,
            bind_addr: self.bind_addr,
            out_ip: None,
            cancel_token: self.client.cancel_token.clone(),
            data_send_to_remote: l2t_receiver,
            data_send_to_local: t2l_sender,
            traffic_collector: None,
            server_name: None,
            piped_stream: None,
            custom_udp_socket: None,
        };

        // Send connection request to private_tun client
        self.client.push_event(conn_type).await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "failed to send UDP connection request",
            )
        })?;

        Ok(Box::new(Datagram {
            l2t_sender,
            t2l_receiver: tokio::sync::Mutex::new(t2l_receiver),
            cancel_token: self.client.cancel_token.clone(),
        }))
    }
}

pub struct Datagram {
    pub l2t_sender: UdpRingBufL2TSender,
    pub t2l_receiver:
        tokio::sync::Mutex<private_tun::snell_impl_ver::udp_intf::UdpRingBufT2LReceiver>,
    pub cancel_token: CancellationToken,
}

impl OutboundDatagram for Datagram {
    fn split(
        self: Box<Self>,
    ) -> (
        Box<dyn OutboundDatagramRecvHalf>,
        Box<dyn OutboundDatagramSendHalf>,
    ) {
        let l2t_sender = Arc::new(tokio::sync::Mutex::new(self.l2t_sender));
        let t2l_receiver = Arc::new(self.t2l_receiver);
        let cancel_token = self.cancel_token.clone();

        (
            Box::new(DatagramRecvHalf {
                t2l_receiver,
                cancel_token: cancel_token.clone(),
            }),
            Box::new(DatagramSendHalf {
                l2t_sender,
                cancel_token,
            }),
        )
    }
}

pub struct DatagramRecvHalf {
    pub t2l_receiver:
        Arc<tokio::sync::Mutex<private_tun::snell_impl_ver::udp_intf::UdpRingBufT2LReceiver>>,
    pub cancel_token: CancellationToken,
}

#[async_trait]
impl OutboundDatagramRecvHalf for DatagramRecvHalf {
    async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocksAddr)> {
        let mut receiver = self.t2l_receiver.lock().await;

        tokio::select! {
            result = receiver.pop() => {
                match result {
                    Some((data, peer_addr)) => {
                        let data_len = data.len();
                        if data_len > buf.len() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "buffer too small for received data",
                            ));
                        }
                        buf[..data_len].copy_from_slice(&data);
                        Ok((data_len, SocksAddr::Ip(peer_addr)))
                    }
                    None => Err(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to receive UDP data: channel closed",
                    )),
                }
            }
            _ = self.cancel_token.cancelled() => {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "UDP receive cancelled",
                ))
            }
        }
    }
}

pub struct DatagramSendHalf {
    pub l2t_sender: Arc<tokio::sync::Mutex<UdpRingBufL2TSender>>,
    pub cancel_token: CancellationToken,
}

#[async_trait]
impl OutboundDatagramSendHalf for DatagramSendHalf {
    async fn send_to(&mut self, buf: &[u8], target: &SocksAddr) -> io::Result<usize> {
        let target_addr = match target {
            SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        let data = private_tun::snell_impl_ver::udp_intf::BytesOrPoolItem::Bytes(
            bytes::Bytes::copy_from_slice(buf),
        );

        let mut sender = self.l2t_sender.lock().await;

        tokio::select! {
            result = sender.push((data, target_addr)) => {
                match result {
                    Ok(()) => Ok(buf.len()),
                    Err(_) => Err(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to send UDP data",
                    )),
                }
            }
            _ = self.cancel_token.cancelled() => {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "UDP send cancelled",
                ))
            }
        }
    }

    async fn close(&mut self) -> io::Result<()> {
        self.l2t_sender.lock().await.close();
        Ok(())
    }
}
