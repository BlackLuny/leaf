use std::sync::atomic::{AtomicBool, Ordering};
use std::{io, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::future::{abortable, AbortHandle};
use futures::FutureExt;
use lru_time_cache::LruCache;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;
use tracing::{debug, trace};

use crate::{app::SyncDnsClient, proxy::*, session::*};

pub struct Handler {
    actors: Vec<AnyOutboundHandler>,
    fail_timeout: u32,
    schedule: Arc<Mutex<Vec<usize>>>,
    health_check_task: Mutex<Option<BoxFuture<'static, ()>>>,
    cache: Option<Arc<Mutex<LruCache<String, usize>>>>,
    last_resort: Option<AnyOutboundHandler>,
    dns_client: SyncDnsClient,
    last_active: Arc<Mutex<Instant>>,
    health_check: bool,
    is_first_health_check_done: Arc<AtomicBool>,
    wait_for_health_check: Option<Arc<Notify>>,
}

impl Handler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        actors: Vec<AnyOutboundHandler>,
        fail_timeout: u32, // in secs
        health_check: bool,
        check_interval: u32, // in secs
        failover: bool,
        fallback_cache: bool,
        cache_size: usize,
        cache_timeout: u64, // in minutes
        last_resort: Option<AnyOutboundHandler>,
        health_check_timeout: u32,
        health_check_delay: u32,
        health_check_active: u32,
        health_check_prefers: Vec<String>,
        health_check_on_start: bool,
        health_check_wait: bool,
        health_check_attempts: u32,
        health_check_success_percentage: u32,
        dns_client: SyncDnsClient,
    ) -> (Self, Vec<AbortHandle>) {
        let mut abort_handles = Vec::new();
        let schedule = Arc::new(Mutex::new((0..actors.len()).collect()));
        let last_active = Arc::new(Mutex::new(Instant::now()));
        let is_first_health_check_done = Arc::new(AtomicBool::new(false));

        let (health_check_task, wait_for_health_check) = if health_check {
            let notify = if health_check_wait {
                Some(Arc::new(Notify::new()))
            } else {
                None
            };
            let (abortable, abort_handle) = abortable(super::health_check_task(
                Network::Tcp,
                schedule.clone(),
                actors.clone(),
                dns_client.clone(),
                check_interval,
                failover,
                last_resort.clone(),
                health_check_timeout,
                health_check_delay,
                health_check_active,
                health_check_prefers,
                last_active.clone(),
                is_first_health_check_done.clone(),
                notify.as_ref().cloned(),
                health_check_attempts,
                health_check_success_percentage,
            ));
            abort_handles.push(abort_handle);
            let task: BoxFuture<'static, ()> = Box::pin(abortable.map(|_| ()));
            if health_check_on_start {
                tokio::spawn(task);
                (Mutex::new(None), notify)
            } else {
                (Mutex::new(Some(task)), notify)
            }
        } else {
            (Mutex::new(None), None)
        };

        let cache = if fallback_cache {
            Some(Arc::new(Mutex::new(
                LruCache::with_expiry_duration_and_capacity(
                    Duration::from_secs(cache_timeout * 60),
                    cache_size,
                ),
            )))
        } else {
            None
        };

        (
            Handler {
                actors,
                fail_timeout,
                schedule,
                health_check_task,
                cache,
                last_resort,
                dns_client,
                last_active,
                health_check,
                is_first_health_check_done,
                wait_for_health_check,
            },
            abort_handles,
        )
    }
}

#[async_trait]
impl OutboundStreamHandler for Handler {
    async fn connect_addr(&self, _sess: &Session) -> OutboundConnect {
        OutboundConnect::Unknown
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        _lhs: Option<&mut AnyStream>,
        _stream: Option<AnyStream>,
    ) -> io::Result<AnyStream> {
        *self.last_active.lock().await = Instant::now();

        if let Some(task) = self.health_check_task.lock().await.take() {
            tokio::spawn(task);
        }

        if self.health_check && !self.is_first_health_check_done.load(Ordering::Relaxed) {
            if let Some(w) = self.wait_for_health_check.as_ref() {
                debug!("holding {}", &sess.destination);
                w.notified().await;
                debug!("{} resumed", &sess.destination);
            }
        }

        if let Some(cache) = &self.cache {
            // Try the cached actor first if exists.
            let cache_key = sess.destination.to_string();
            if let Some(idx) = cache.lock().await.get(&cache_key) {
                let a = &self.actors[*idx];
                debug!(
                    "failover handles tcp [{}] to cached [{}]",
                    sess.destination,
                    a.tag()
                );
                // TODO Remove the entry immediately if timeout or fail?
                if let Ok(Ok(v)) = timeout(
                    Duration::from_secs(self.fail_timeout as u64),
                    a.stream()?.handle(
                        sess,
                        None,
                        connect_stream_outbound(sess, self.dns_client.clone(), a).await?,
                    ),
                )
                .await
                {
                    return Ok(v);
                }
            };
        }

        let schedule = self.schedule.lock().await.clone();

        // Use the last resort outbound if all outbounds have failed in
        // the last health check.
        if schedule.is_empty() && self.last_resort.is_some() {
            let a = &self.last_resort.as_ref().unwrap();
            debug!(
                "failover handles tcp [{}] to last resort [{}]",
                sess.destination,
                a.tag()
            );
            return a
                .stream()?
                .handle(
                    sess,
                    None,
                    connect_stream_outbound(sess, self.dns_client.clone(), a).await?,
                )
                .await;
        }

        for (sche_idx, actor_idx) in schedule.into_iter().enumerate() {
            if actor_idx >= self.actors.len() {
                return Err(io::Error::new(io::ErrorKind::Other, "invalid actor index"));
            }

            let a = &self.actors[actor_idx];

            debug!(
                "[{}] handles [{}:{}] to [{}]",
                a.tag(),
                sess.network,
                sess.destination,
                a.tag()
            );

            let try_outbound = async move {
                a.stream()?
                    .handle(
                        sess,
                        None,
                        connect_stream_outbound(sess, self.dns_client.clone(), a).await?,
                    )
                    .await
            };

            match timeout(Duration::from_secs(self.fail_timeout as u64), try_outbound).await {
                Ok(t) => match t {
                    Ok(v) => {
                        // Only cache for fallback actors.
                        if let Some(cache) = &self.cache {
                            if sche_idx > 0 {
                                let cache_key = sess.destination.to_string();
                                trace!("failover inserts {} -> {} to cache", cache_key, a.tag());
                                cache.lock().await.insert(cache_key, actor_idx);
                            }
                        }
                        return Ok(v);
                    }
                    Err(e) => {
                        trace!(
                            "[{}] failed to handle [{}:{}]: {}",
                            a.tag(),
                            sess.network,
                            sess.destination,
                            e,
                        );
                        continue;
                    }
                },
                Err(e) => {
                    trace!(
                        "[{}] failed to handle [{}:{}]: {}",
                        a.tag(),
                        sess.network,
                        sess.destination,
                        e,
                    );
                    continue;
                }
            }
        }

        if let Some(a) = self.last_resort.as_ref() {
            debug!(
                "failover handles tcp [{}] to last resort [{}]",
                sess.destination,
                a.tag()
            );
            return a
                .stream()?
                .handle(
                    sess,
                    None,
                    connect_stream_outbound(sess, self.dns_client.clone(), a).await?,
                )
                .await;
        }

        Err(io::Error::new(
            io::ErrorKind::Other,
            "all outbound attempts failed",
        ))
    }
}
