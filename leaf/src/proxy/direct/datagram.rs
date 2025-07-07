use std::io;

use async_trait::async_trait;

use crate::{proxy::*, session::Session};

pub struct Handler;

#[async_trait]
impl OutboundDatagramHandler for Handler {
    async fn connect_addr(&self, _sess: &Session) -> OutboundConnect {
        OutboundConnect::Direct
    }

    fn transport_type(&self) -> DatagramTransportType {
        DatagramTransportType::Unreliable
    }

    async fn handle<'a>(
        &'a self,
        _sess: &'a Session,
        transport: Option<AnyOutboundTransport>,
    ) -> io::Result<AnyOutboundDatagram> {
        if let Some(OutboundTransport::Datagram(dgram)) = transport {
            Ok(dgram)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "invalid input"))
        }
    }
}
