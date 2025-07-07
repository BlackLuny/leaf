use std::io;

use async_ringbuf::traits::AsyncProducer;
use async_trait::async_trait;
use futures::TryFutureExt;
use private_tun::snell_impl_ver::{client_zfc::ConnType, udp_intf::AsyncRingBufSender};
use tokio::io::duplex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::{
    proxy::{AnyStream, Color, OutboundConnect, OutboundStreamHandler, Tag, TcpConnector},
    session::{Network, Session},
};

pub struct Handler {
    pub tag: String,
    pub color: colored::Color,
    // Channel to send connection requests to private_tun client
    pub client_tx: tokio::sync::Mutex<AsyncRingBufSender<ConnType>>,
    pub cancel_token: tokio_util::sync::CancellationToken,
}
impl Drop for Handler {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
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
    fn connect_addr(&self) -> OutboundConnect {
        OutboundConnect::Unknown
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        _lhs: Option<&mut AnyStream>,
        stream: Option<AnyStream>,
    ) -> io::Result<AnyStream> {
        let stream =
            stream.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing stream"))?;

        // Create a duplex channel to bridge leaf's stream with private_tun
        let (client_side, server_side) = duplex(8192);

        // Create MemDuplex using private_tun's implementation
        let mem_duplex =
            private_tun::self_proxy::MemDuplex::new(client_side, self.cancel_token.clone());

        // Convert destination to private_tun's Address format
        let target = match &sess.destination {
            crate::session::SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            crate::session::SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        // Create oneshot channel for response
        let (rst_tx, rst_rx) = oneshot::channel();

        // Create ConnType for Duplex connection
        let conn_type = private_tun::snell_impl_ver::client_zfc::ConnType::Duplex {
            stream: mem_duplex,
            target,
            rst_tx,
            server_name: None,
            traffic_collector: None,
            one_rtt: false,
            reuse_tcp: true,
        };

        // Send connection request to private_tun client
        self.client_tx.lock().await.push(conn_type).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("failed to send connection request"),
            )
        })?;

        Ok(Box::new(server_side))
    }
}
