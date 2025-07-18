use std::convert::TryFrom;
use std::io;

use async_trait::async_trait;

use crate::{
    proxy::*,
    session::{Session, SocksAddr},
};

pub struct Handler {
    pub actors: Vec<AnyOutboundHandler>,
}

impl Handler {
    async fn next_connect_addr(&self, start: usize, sess: &Session) -> OutboundConnect {
        for a in self.actors[start..].iter() {
            match a.stream() {
                Ok(h) => {
                    let oc = h.connect_addr(sess).await;
                    if let OutboundConnect::Next = oc {
                        continue;
                    }
                    return oc;
                }
                _ => {
                    if let Ok(h) = a.datagram() {
                        let oc = h.connect_addr(sess).await;
                        if let OutboundConnect::Next = oc {
                            continue;
                        }
                        return oc;
                    }
                }
            }
        }
        OutboundConnect::Unknown
    }

    async fn next_session(&self, mut sess: Session, start: usize) -> Session {
        if let OutboundConnect::Proxy(_, address, port) = self.next_connect_addr(start, &sess).await
        {
            if let Ok(addr) = SocksAddr::try_from((address, port)) {
                sess.destination = addr;
            }
        }
        sess
    }
}

#[async_trait]
impl OutboundStreamHandler for Handler {
    async fn connect_addr(&self, sess: &Session) -> OutboundConnect {
        self.next_connect_addr(0, sess).await
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        mut lhs: Option<&mut AnyStream>,
        mut stream: Option<AnyStream>,
    ) -> io::Result<AnyStream> {
        for (i, a) in self.actors.iter().enumerate() {
            let new_sess = self.next_session(sess.clone(), i + 1).await;
            let s = stream.take();
            let lhs_stream = if i == self.actors.len() - 1 {
                lhs.take()
            } else {
                None
            };
            stream.replace(a.stream()?.handle(&new_sess, lhs_stream, s).await?);
        }
        Ok(stream
            .map(Box::new)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "chain tcp invalid input"))?)
    }
}
