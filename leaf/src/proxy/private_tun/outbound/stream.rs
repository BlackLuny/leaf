use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use async_ringbuf::traits::AsyncProducer;
use async_trait::async_trait;
use private_tun::snell_impl_ver::{
    client_run::ProxyReportResult,
    client_zfc::{AnyClientStream, ClientStream, ConnType},
};
use tokio::io::{duplex, AsyncRead, AsyncWrite, ReadBuf};
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
        OutboundConnect::Unknown
    }

    fn relay_by_my_self(&self) -> bool {
        true
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        _lhs: Option<&mut AnyStream>,
        stream: Option<AnyStream>,
    ) -> io::Result<AnyStream> {
        // Create a duplex channel to bridge leaf's stream with private_tun
        let (client_side_pipe0, client_side_pipe1) = duplex(2048);
        // let (server_side_pipe0, mut server_side_pipe1) = duplex(8192);

        // Create MemDuplex using private_tun's implementation
        let mem_duplex = private_tun::self_proxy::MemDuplex::new(
            client_side_pipe0,
            self.client.cancel_token.clone(),
        );

        // Convert destination to private_tun's Address format
        let target = match &sess.destination {
            crate::session::SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            crate::session::SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        // Create oneshot channel for response
        let (rst_tx, _rst_rx) = oneshot::channel();

        // Create ConnType for Duplex connection
        let conn_type = ConnType::Duplex {
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

    async fn handle_by_my_self<'a>(
        &'a self,
        sess: &'a Session,
        lhs: AnyStream,
        stream: Option<AnyStream>,
    ) -> io::Result<()> {
        struct AnyStreamWrapper(AnyStream);
        impl ClientStream for AnyStreamWrapper {}
        impl AsyncRead for AnyStreamWrapper {
            fn poll_read(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.0).poll_read(cx, buf)
            }
        }

        impl AsyncWrite for AnyStreamWrapper {
            fn poll_write(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Pin::new(&mut self.0).poll_write(cx, buf)
            }
            fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                Pin::new(&mut self.0).poll_flush(cx)
            }
            fn poll_shutdown(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<io::Result<()>> {
                Pin::new(&mut self.0).poll_shutdown(cx)
            }
        }
        let client_stream = AnyClientStream::new(Box::new(AnyStreamWrapper(lhs)));
        let target = match &sess.destination {
            crate::session::SocksAddr::Ip(addr) => private_tun::address::Address::Socket(*addr),
            crate::session::SocksAddr::Domain(domain, port) => {
                private_tun::address::Address::Domain(domain.clone().into(), *port)
            }
        };

        // Create oneshot channel for response
        let (rst_tx, rst_rx) = oneshot::channel();

        // Create ConnType for Duplex connection
        let conn_type = ConnType::AnyStream {
            stream: client_stream,
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

        let rst = rst_rx.await.unwrap();
        match rst.error {
            Some(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
            None => Ok(()),
        }
    }
}
