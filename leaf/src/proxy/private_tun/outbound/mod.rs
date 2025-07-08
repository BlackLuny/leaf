pub mod datagram;
pub mod stream;

use std::sync::atomic::Ordering;

use async_ringbuf::traits::AsyncProducer;
pub use datagram::Handler as DatagramHandler;
use private_tun::snell_impl_ver::{client_zfc::ConnType, udp_intf::AsyncRingBufSender};
pub use stream::Handler;
use tokio_util::sync::CancellationToken;

pub struct Client {
    pub client_tx: tokio::sync::Mutex<AsyncRingBufSender<ConnType>>,
    pub cancel_token: CancellationToken,
    pub notifier: Arc<tokio::sync::Notify>,
    pub is_started: AtomicBool,
}

impl Client {
    pub fn new(
        client_tx: tokio::sync::Mutex<AsyncRingBufSender<ConnType>>,
        cancel_token: CancellationToken,
        notifier: Arc<tokio::sync::Notify>,
    ) -> Self {
        Self {
            client_tx,
            cancel_token,
            notifier,
            is_started: AtomicBool::new(false),
        }
    }
    pub async fn push_event(&self, event: ConnType) -> Result<(), ConnType> {
        if !self.is_started.load(Ordering::Relaxed) {
            self.notifier.notify_one();
            self.is_started.store(true, Ordering::Relaxed);
        }
        self.client_tx.lock().await.push(event).await?;
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
