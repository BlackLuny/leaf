use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::{proxy::*, session::Session};

pub struct Handler {
    pub actors: Vec<AnyOutboundHandler>,
    pub selected: Arc<AtomicUsize>,
}

#[async_trait]
impl OutboundDatagramHandler for Handler {
    async fn connect_addr(&self, sess: &Session) -> OutboundConnect {
        let a = &self.actors[self.selected.load(Ordering::Relaxed)];
        match a.datagram() {
            Ok(h) => return h.connect_addr(sess).await,
            _ => match a.stream() {
                Ok(h) => return h.connect_addr(sess).await,
                _ => (),
            },
        }
        OutboundConnect::Unknown
    }

    fn transport_type(&self) -> DatagramTransportType {
        let a = &self.actors[self.selected.load(Ordering::Relaxed)];
        a.datagram()
            .map(|x| x.transport_type())
            .unwrap_or(DatagramTransportType::Unknown)
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        transport: Option<AnyOutboundTransport>,
    ) -> io::Result<AnyOutboundDatagram> {
        let a = &self.actors[self.selected.load(Ordering::Relaxed)];
        tracing::debug!("select handles to [{}]", a.tag());
        a.datagram()?.handle(sess, transport).await
    }
}
