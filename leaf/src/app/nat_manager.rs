use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use futures::future::{abortable, BoxFuture};
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot, Mutex, MutexGuard,
};
use tracing::{debug, error, trace};

use crate::app::dispatcher::Dispatcher;
use crate::option;
use crate::session::{DatagramSource, Network, Session, SocksAddr};

#[derive(Debug)]
pub struct UdpPacket {
    pub data: Vec<u8>,
    pub src_addr: SocksAddr,
    pub dst_addr: SocksAddr,
}

impl UdpPacket {
    pub fn new(data: Vec<u8>, src_addr: SocksAddr, dst_addr: SocksAddr) -> Self {
        Self {
            data,
            src_addr,
            dst_addr,
        }
    }
}

impl std::fmt::Display for UdpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} <-> {}, {} bytes",
            self.src_addr,
            self.dst_addr,
            self.data.len()
        )
    }
}

type SessionMap = DashMap<DatagramSource, (Sender<UdpPacket>, oneshot::Sender<bool>, Instant)>;

pub struct NatManager {
    sessions: Arc<SessionMap>,
    dispatcher: Arc<Dispatcher>,
    timeout_check_task: Mutex<Option<BoxFuture<'static, ()>>>,
}

impl NatManager {
    pub fn new(dispatcher: Arc<Dispatcher>) -> Self {
        let sessions: Arc<SessionMap> = Arc::new(SessionMap::new());
        let sessions2 = sessions.clone();

        // The task is lazy, will not run until any sessions added.
        let timeout_check_task: BoxFuture<'static, ()> = Box::pin(async move {
            loop {
                let now = Instant::now();
                let mut to_be_remove = Vec::new();
                for kv in sessions2.iter() {
                    let (key, val) = kv.pair();
                    if now.duration_since(val.2).as_secs() >= *option::UDP_SESSION_TIMEOUT {
                        to_be_remove.push(key.to_owned());
                    }
                }
                for key in to_be_remove.iter() {
                    if let Some((_, mut sess)) = sessions2.remove(key) {
                        // Sends a signal to abort downlink task, uplink task will
                        // end automatically when we drop the channel's tx side upon
                        // session removal.
                        if let Err(e) = sess.1.send(true) {
                            debug!("failed to send abort signal on session {}: {}", key, e);
                        }
                        debug!("udp session {} ended", key);
                    }
                }
                drop(to_be_remove); // drop explicitly
                tokio::time::sleep(Duration::from_secs(
                    *option::UDP_SESSION_TIMEOUT_CHECK_INTERVAL,
                ))
                .await;
            }
        });

        NatManager {
            sessions,
            dispatcher,
            timeout_check_task: Mutex::new(Some(timeout_check_task)),
        }
    }

    async fn send_to(&self, sender: &mut Sender<UdpPacket>, instant: &mut Instant, pkt: UdpPacket) {
        if let Err(err) = sender.send(pkt).await {
            trace!("send uplink packet failed {}", err);
        }
        *instant = Instant::now(); // activity update
    }

    pub async fn send<'a>(
        &self,
        sess: Option<&Session>,
        dgram_src: &DatagramSource,
        inbound_tag: &str,
        client_ch_tx: &Sender<UdpPacket>,
        pkt: UdpPacket,
    ) {
        if let Some(mut entry) = self.sessions.get_mut(dgram_src) {
            let (sender, _, instant) = entry.value_mut();
            self.send_to(sender, instant, pkt).await;
            return;
        }

        let mut sess = sess.cloned().unwrap_or(Session {
            network: Network::Udp,
            source: dgram_src.address,
            destination: pkt.dst_addr.clone(),
            inbound_tag: inbound_tag.to_string(),
            ..Default::default()
        });
        if sess.inbound_tag.is_empty() {
            sess.inbound_tag = inbound_tag.to_string();
        }

        self.add_session(sess, *dgram_src, client_ch_tx.clone(), &self.sessions)
            .await;

        debug!(
            "added udp session {} -> {} ({})",
            &dgram_src,
            &pkt.dst_addr,
            self.sessions.len(),
        );

        if let Some(mut entry) = self.sessions.get_mut(dgram_src) {
            let (sender, _, instant) = entry.value_mut();
            self.send_to(sender, instant, pkt).await;
            return;
        }
    }

    pub async fn add_session<'a>(
        &self,
        sess: Session,
        raddr: DatagramSource,
        client_ch_tx: Sender<UdpPacket>,
        session_map: &SessionMap,
    ) {
        // Runs the lazy task for session cleanup job, this task will run only once.
        if let Some(task) = self.timeout_check_task.lock().await.take() {
            tokio::spawn(task);
        }

        let (target_ch_tx, mut target_ch_rx) =
            mpsc::channel(*crate::option::UDP_UPLINK_CHANNEL_SIZE);
        let (downlink_abort_tx, downlink_abort_rx) = oneshot::channel();

        session_map.insert(raddr, (target_ch_tx, downlink_abort_tx, Instant::now()));

        let dispatcher = self.dispatcher.clone();
        let sessions = self.sessions.clone();

        // Spawns a new task for dispatching to avoid blocking the current task,
        // because we have stream type transports for UDP traffic, establishing a
        // TCP stream would block the task.
        tokio::spawn(async move {
            // new socket to communicate with the target.
            let socket = match dispatcher.dispatch_datagram(sess).await {
                Ok(s) => s,
                Err(e) => {
                    debug!("dispatch {} failed: {}", &raddr, e);
                    sessions.remove(&raddr);
                    return;
                }
            };

            let (mut target_sock_recv, mut target_sock_send) = socket.split();

            // downlink
            let downlink_task = async move {
                let mut buf = vec![0u8; *crate::option::DATAGRAM_BUFFER_SIZE * 1024];
                loop {
                    match target_sock_recv.recv_from(&mut buf).await {
                        Err(err) => {
                            debug!(
                                "Failed to receive downlink packets on session {}: {}",
                                &raddr, err
                            );
                            break;
                        }
                        Ok((n, addr)) => {
                            trace!("outbound received UDP packet: src {}, {} bytes", &addr, n);
                            let pkt = UdpPacket::new(
                                buf[..n].to_vec(),
                                addr.clone(),
                                SocksAddr::from(raddr.address),
                            );
                            if let Err(err) = client_ch_tx.send(pkt).await {
                                debug!(
                                    "Failed to send downlink packets on session {} to {}: {}",
                                    &raddr, &addr, err
                                );
                                break;
                            }

                            // activity update
                            {
                                if let Some(mut sess) = sessions.get_mut(&raddr) {
                                    if addr.port() == 53 {
                                        // If the destination port is 53, we assume it's a
                                        // DNS query and set a negative timeout so it will
                                        // be removed on next check.
                                        sess.2.checked_sub(Duration::from_secs(
                                            *option::UDP_SESSION_TIMEOUT,
                                        ));
                                    } else {
                                        sess.2 = Instant::now();
                                    }
                                }
                            }
                        }
                    }
                }
                sessions.remove(&raddr);
            };

            let (downlink_task, downlink_task_handle) = abortable(downlink_task);
            tokio::spawn(downlink_task);

            // Runs a task to receive the abort signal.
            tokio::spawn(async move {
                let _ = downlink_abort_rx.await;
                downlink_task_handle.abort();
            });

            // uplink
            tokio::spawn(async move {
                while let Some(pkt) = target_ch_rx.recv().await {
                    trace!(
                        "outbound send UDP packet: dst {}, {} bytes",
                        &pkt.dst_addr,
                        pkt.data.len()
                    );
                    if let Err(e) = target_sock_send.send_to(&pkt.data, &pkt.dst_addr).await {
                        debug!(
                            "Failed to send uplink packets on session {} to {}: {:?}",
                            &raddr, &pkt.dst_addr, e
                        );
                        break;
                    }
                }
                if let Err(e) = target_sock_send.close().await {
                    debug!("Failed to close outbound datagram {}: {}", &raddr, e);
                }
            });
        });
    }
}
