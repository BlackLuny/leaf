use std::convert::TryFrom;
use std::io::{self, ErrorKind};
use std::sync::Arc;
use std::time::Duration;

use ::private_tun::self_proxy::Closeable;
use async_recursion::async_recursion;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::select;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    app::SyncDnsClient,
    common::{self, sniff},
    option,
    proxy::*,
    session::*,
};

#[cfg(feature = "stat")]
use crate::app::SyncStatManager;

use super::outbound::manager::OutboundManager;
use super::router::Router;

#[inline]
fn log_request(
    sess: &Session,
    outbound_tag: &str,
    outbound_tag_color: &colored::Color,
    handshake_time: Option<u128>,
) {
    let hs = handshake_time.map_or("failed".to_string(), |hs| format!("{}ms", hs));
    let (network, outbound_tag) = if !*crate::option::LOG_NO_COLOR {
        use colored::Colorize;
        let network_color = match sess.network {
            Network::Tcp => colored::Color::Blue,
            Network::Udp => colored::Color::Yellow,
        };
        (
            sess.network.to_string().color(network_color).to_string(),
            outbound_tag.color(*outbound_tag_color).to_string(),
        )
    } else {
        (sess.network.to_string(), outbound_tag.to_string())
    };
    info!(
        "[{}] [{}] [{}] [{}] [{}] [{}]",
        sess.forwarded_source.unwrap_or_else(|| sess.source.ip()),
        network,
        &sess.inbound_tag,
        outbound_tag,
        hs,
        &sess.destination,
    );
}

pub struct Dispatcher {
    outbound_manager: Arc<RwLock<OutboundManager>>,
    router: Arc<RwLock<Router>>,
    dns_client: SyncDnsClient,
    #[cfg(feature = "stat")]
    stat_manager: SyncStatManager,
    all_session_cancel_tokens: Arc<RwLock<Vec<CancellationToken>>>,
    handle_session_cancel_task: JoinHandle<()>,
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        self.handle_session_cancel_task.abort();
    }
}

impl Dispatcher {
    pub fn new(
        outbound_manager: Arc<RwLock<OutboundManager>>,
        router: Arc<RwLock<Router>>,
        dns_client: SyncDnsClient,
        #[cfg(feature = "stat")] stat_manager: SyncStatManager,
    ) -> Self {
        let all_session_cancel_tokens: Arc<RwLock<Vec<CancellationToken>>> =
            Arc::new(RwLock::new(Vec::new()));
        let all_session_cancel_tokens_clone = all_session_cancel_tokens.clone();
        let handle_session_cancel_task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let mut cancel_tokens = all_session_cancel_tokens_clone.write().await;
                cancel_tokens.retain(|token| !token.is_cancelled());
            }
        });
        Dispatcher {
            outbound_manager,
            router,
            dns_client,
            #[cfg(feature = "stat")]
            stat_manager,
            all_session_cancel_tokens,
            handle_session_cancel_task,
        }
    }

    pub async fn dispatch_stream<T>(&self, mut sess: Session, lhs: T)
    where
        T: 'static + AsyncRead + AsyncWrite + Unpin + Send + Sync,
    {
        debug!("dispatching {}:{}", &sess.network, &sess.destination);
        let mut lhs: Box<dyn ProxyStream> = if sniff::should_sniff(&sess) {
            let mut lhs = sniff::SniffingStream::new(lhs);
            match lhs.sniff(&sess).await {
                Ok(res) => {
                    if let Some(domain) = res {
                        debug!(
                            "sniffed domain {} for tcp link {} <-> {}",
                            &domain, &sess.source, &sess.destination,
                        );
                        // TODO Add an option to use the sniffed domain for routing only
                        //
                        // TODO Add DNS sniff, sniff domain name from DNS response, keep
                        // an IP -> domain mapping, use this info for routing only.
                        sess.destination =
                            match SocksAddr::try_from((&domain, sess.destination.port())) {
                                Ok(a) => a,
                                Err(e) => {
                                    warn!(
                                        "convert sniffed domain {} to destination failed: {}",
                                        &domain, e,
                                    );
                                    return;
                                }
                            };
                    }
                }
                Err(e) => {
                    debug!(
                        "sniff tcp uplink {} -> {} failed: {}",
                        &sess.source, &sess.destination, e,
                    );
                    return;
                }
            }
            Box::new(lhs)
        } else {
            Box::new(lhs)
        };

        let outbound = {
            let router = self.router.read().await;
            match router.pick_route(&sess).await {
                Ok(tag) => {
                    debug!(
                        "picked route [{}] for {} -> {}",
                        tag, &sess.source, &sess.destination
                    );
                    tag.to_owned()
                }
                Err(err) => {
                    debug!("pick route failed: {}", err);
                    if let Some(tag) = self.outbound_manager.read().await.default_handler() {
                        debug!(
                            "picked default route [{}] for {} -> {}",
                            tag, &sess.source, &sess.destination
                        );
                        tag
                    } else {
                        warn!("can not find any handlers");
                        return;
                    }
                }
            }
        };

        sess.outbound_tag = outbound.clone();

        let h = if let Some(h) = self.outbound_manager.read().await.get(&outbound) {
            h
        } else {
            // FIXME use  the default handler
            warn!("handler not found");
            return;
        };
        debug!(
            "handling {}:{} with {}",
            &sess.network,
            &sess.destination,
            h.tag()
        );

        let handshake_start = tokio::time::Instant::now();
        let stream =
            match crate::proxy::connect_stream_outbound(&sess, self.dns_client.clone(), &h).await {
                Ok(s) => s,
                Err(e) => {
                    debug!(
                        "dispatch tcp {} -> {} to [{}] failed: {}",
                        &sess.source,
                        &sess.destination,
                        &h.tag(),
                        e
                    );
                    log_request(&sess, h.tag(), h.color(), None);
                    return;
                }
            };
        let th = match h.stream() {
            Ok(th) => th,
            Err(e) => {
                warn!(
                    "dispatch tcp {} -> {} to [{}] failed: {}",
                    &sess.source,
                    &sess.destination,
                    &h.tag(),
                    e
                );
                return;
            }
        };
        if th.relay_by_my_self() {
            let cancel_token = CancellationToken::new();
            self.all_session_cancel_tokens
                .write()
                .await
                .push(cancel_token.clone());
            lhs = Box::new(Closeable::new(lhs, cancel_token.clone())) as AnyStream;
            #[cfg(feature = "stat")]
            if *crate::option::ENABLE_STATS {
                lhs = self
                    .stat_manager
                    .write()
                    .await
                    .stat_stream_l(lhs, sess.clone());
            }

            match th.handle_by_my_self(&sess, lhs, stream).await {
                Ok(()) => {
                    debug!("tcp link {} <-> {} done", &sess.source, &sess.destination);
                    log_request(&sess, h.tag(), h.color(), None);
                }
                Err(e) => {
                    debug!(
                        "tcp link {} <-> {} error: {} [{}]",
                        &sess.source,
                        &sess.destination,
                        e,
                        &h.tag()
                    );
                    log_request(&sess, h.tag(), h.color(), None);
                }
            }
            return;
        }
        match th.handle(&sess, Some(&mut lhs), stream).await {
            Ok(mut rhs) => {
                let elapsed = tokio::time::Instant::now().duration_since(handshake_start);

                log_request(&sess, h.tag(), h.color(), Some(elapsed.as_millis()));

                #[cfg(feature = "stat")]
                if *crate::option::ENABLE_STATS {
                    rhs = self
                        .stat_manager
                        .write()
                        .await
                        .stat_stream(rhs, sess.clone());
                }
                let cancel_token = CancellationToken::new();
                self.all_session_cancel_tokens
                    .write()
                    .await
                    .push(cancel_token.clone());
                let _cancel_guard = cancel_token.clone().drop_guard();
                select! {
                    biased;
                    r = common::io::copy_buf_bidirectional_with_timeout(
                        &mut lhs,
                        &mut rhs,
                        *option::LINK_BUFFER_SIZE * 1024,
                        Duration::from_secs(*option::TCP_UPLINK_TIMEOUT),
                        Duration::from_secs(*option::TCP_DOWNLINK_TIMEOUT),
                    ) => {
                        match r {
                            Ok((up_count, down_count)) => {
                                debug!(
                                    "tcp link {} <-> {} done, ({}, {}) bytes transferred [{}]",
                                    &sess.source,
                                    &sess.destination,
                                    up_count,
                                    down_count,
                                    &h.tag(),
                                );
                            }
                            Err(e) => {
                                debug!(
                                    "tcp link {} <-> {} error: {} [{}]",
                                    &sess.source,
                                    &sess.destination,
                                    e,
                                    &h.tag()
                                );
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        info!("tcp link {} <-> {} cancelled", &sess.source, &sess.destination);
                    }
                }
            }
            Err(e) => {
                debug!(
                    "dispatch tcp {} -> {} to [{}] failed: {}",
                    &sess.source,
                    &sess.destination,
                    &h.tag(),
                    e
                );
                log_request(&sess, h.tag(), h.color(), None);
            }
        }
    }

    #[async_recursion]
    pub async fn dispatch_datagram(
        &self,
        mut sess: Session,
    ) -> io::Result<Box<dyn OutboundDatagram>> {
        debug!("dispatching {}:{}", &sess.network, &sess.destination);
        let outbound = {
            let router = self.router.read().await;
            match router.pick_route(&sess).await {
                Ok(tag) => {
                    debug!(
                        "picked route [{}] for {} -> {}",
                        tag, &sess.source, &sess.destination
                    );
                    tag.to_owned()
                }
                Err(err) => {
                    debug!("pick route failed: {}", err);
                    if let Some(tag) = self.outbound_manager.read().await.default_handler() {
                        debug!(
                            "picked default route [{}] for {} -> {}",
                            tag, &sess.source, &sess.destination
                        );
                        tag
                    } else {
                        warn!("no handler found");
                        return Err(io::Error::new(ErrorKind::Other, "no available handler"));
                    }
                }
            }
        };

        sess.outbound_tag = outbound.clone();

        let h = if let Some(h) = self.outbound_manager.read().await.get(&outbound) {
            h
        } else {
            warn!("handler not found");
            return Err(io::Error::new(ErrorKind::Other, "handler not found"));
        };

        let handshake_start = tokio::time::Instant::now();
        let transport =
            crate::proxy::connect_datagram_outbound(&sess, self.dns_client.clone(), &h).await?;
        debug!(
            "handling {}:{} with {}",
            &sess.network,
            &sess.destination,
            h.tag()
        );
        let cancel_token = CancellationToken::new();
        self.all_session_cancel_tokens
            .write()
            .await
            .push(cancel_token.clone());
        match h.datagram()?.handle(&sess, transport).await {
            #[allow(unused_mut)]
            Ok(mut d) => {
                let elapsed = tokio::time::Instant::now().duration_since(handshake_start);

                log_request(&sess, h.tag(), h.color(), Some(elapsed.as_millis()));

                #[cfg(feature = "stat")]
                if *crate::option::ENABLE_STATS {
                    d = self
                        .stat_manager
                        .write()
                        .await
                        .stat_outbound_datagram(d, sess.clone());
                }
                let d = CancelableOutboundDatagram::new(d, cancel_token.clone());
                Ok(Box::new(d) as AnyOutboundDatagram)
            }
            Err(e) => {
                debug!(
                    "dispatch udp {} -> {} to [{}] failed: {}",
                    &sess.source,
                    &sess.destination,
                    &h.tag(),
                    e
                );
                log_request(&sess, h.tag(), h.color(), None);
                Err(e)
            }
        }
    }

    pub async fn cancel_all_sessions(&self) {
        let mut cancel_tokens = self.all_session_cancel_tokens.write().await;
        for cancel_token in cancel_tokens.iter() {
            cancel_token.cancel();
        }
        cancel_tokens.clear();
    }
}
