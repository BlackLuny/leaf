use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::anyhow;
use lazy_static::lazy_static;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tracing::debug;
use tracing::{error, info, trace, warn};

#[cfg(feature = "auto-reload")]
use notify::{
    event, Error as NotifyError, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};

use app::{
    dispatcher::Dispatcher, dns_client::DnsClient, inbound::manager::InboundManager,
    nat_manager::NatManager, outbound::manager::OutboundManager, router::Router,
};

use crate::app::outbound::manager::OutBoundHandlerInfo;

#[cfg(feature = "stat")]
use crate::app::{stat_manager::StatManager, SyncStatManager};

#[cfg(feature = "api")]
use crate::app::api::api_server::ApiServer;
use crate::proxy::failover::Measure;

pub mod app;
pub mod common;
pub mod config;
pub mod option;
pub mod proxy;
pub mod session;
pub mod util;

#[cfg(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "android",
    target_vendor = "uwp"
))]
pub mod mobile;

#[cfg(all(feature = "inbound-tun", any(target_os = "macos", target_os = "linux")))]
mod sys;

#[cfg(all(feature = "inbound-tun", target_os = "windows"))]
mod winsys;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] anyhow::Error),
    #[error("no associated config file")]
    NoConfigFile,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[cfg(feature = "auto-reload")]
    #[error(transparent)]
    Watcher(#[from] NotifyError),
    #[error(transparent)]
    AsyncChannelSend(
        #[from] tokio::sync::mpsc::error::SendError<std::sync::mpsc::SyncSender<Result<(), Error>>>,
    ),
    #[error(transparent)]
    SyncChannelRecv(#[from] std::sync::mpsc::RecvError),
    #[error("runtime manager error")]
    RuntimeManager,
}

pub type Runner = futures::future::BoxFuture<'static, ()>;

pub struct RuntimeManager {
    #[cfg(feature = "auto-reload")]
    rt_id: RuntimeId,
    config_path: Option<String>,
    #[cfg(feature = "auto-reload")]
    auto_reload: bool,
    reload_tx: mpsc::Sender<std::sync::mpsc::SyncSender<Result<(), Error>>>,
    shutdown_tx: mpsc::Sender<()>,
    router: Arc<RwLock<Router>>,
    dns_client: Arc<RwLock<DnsClient>>,
    outbound_manager: Arc<RwLock<OutboundManager>>,
    #[cfg(feature = "stat")]
    stat_manager: SyncStatManager,
    #[cfg(feature = "auto-reload")]
    watcher: Mutex<Option<RecommendedWatcher>>,
    dispatcher: Arc<Dispatcher>,
}

impl RuntimeManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        #[cfg(feature = "auto-reload")] rt_id: RuntimeId,
        config_path: Option<String>,
        #[cfg(feature = "auto-reload")] auto_reload: bool,
        reload_tx: mpsc::Sender<std::sync::mpsc::SyncSender<Result<(), Error>>>,
        shutdown_tx: mpsc::Sender<()>,
        router: Arc<RwLock<Router>>,
        dns_client: Arc<RwLock<DnsClient>>,
        outbound_manager: Arc<RwLock<OutboundManager>>,
        #[cfg(feature = "stat")] stat_manager: SyncStatManager,
        dispatcher: Arc<Dispatcher>,
    ) -> Arc<Self> {
        Arc::new(Self {
            #[cfg(feature = "auto-reload")]
            rt_id,
            config_path,
            #[cfg(feature = "auto-reload")]
            auto_reload,
            reload_tx,
            shutdown_tx,
            router,
            dns_client,
            outbound_manager,
            #[cfg(feature = "stat")]
            stat_manager,
            #[cfg(feature = "auto-reload")]
            watcher: Mutex::new(None),
            dispatcher,
        })
    }

    #[cfg(feature = "stat")]
    pub fn stat_manager(&self) -> SyncStatManager {
        self.stat_manager.clone()
    }

    #[cfg(feature = "outbound-select")]
    pub async fn set_outbound_selected(&self, outbound: &str, select: &str) -> Result<(), Error> {
        if let Some(selector) = self.outbound_manager.read().await.get_selector(outbound) {
            selector
                .write()
                .await
                .set_selected(select)
                .map_err(Error::Config)
        } else {
            Err(Error::Config(anyhow!("selector not found")))
        }
    }

    #[cfg(feature = "outbound-select")]
    pub async fn get_outbound_selected(&self, outbound: &str) -> Result<String, Error> {
        let selector = self.outbound_manager.read().await.get_selector(outbound);
        if let Some(selector) = selector {
            return Ok(selector.read().await.get_selected_tag());
        }
        Err(Error::Config(anyhow!("not found")))
    }

    #[cfg(feature = "outbound-select")]
    pub async fn get_outbound_selects(&self, outbound: &str) -> Result<Vec<String>, Error> {
        let selector = self.outbound_manager.read().await.get_selector(outbound);
        if let Some(selector) = selector {
            return Ok(selector.read().await.get_available_tags());
        }
        Err(Error::Config(anyhow!("not found")))
    }

    #[cfg(feature = "outbound-select")]
    pub async fn get_all_outbound_selects(&self) -> Result<Vec<(String, Vec<String>)>, Error> {
        let all_selectable = self
            .outbound_manager
            .read()
            .await
            .get_selectable_outbounds();
        let mut result = Vec::new();
        for tag in all_selectable {
            let selects = self.get_outbound_selects(&tag).await?;
            result.push((tag, selects));
        }
        Ok(result)
    }

    pub async fn set_global_target(&self, target: Option<String>) -> bool {
        tracing::info!("set_global_target: {:?} before get lock", target);
        let mut lock = self.router.write().await;
        tracing::info!("set_global_target: {:?} after get lock", target);
        lock.set_global_target(target)
    }

    pub async fn get_global_target(&self) -> Result<Option<String>, Error> {
        let target = self.router.read().await.get_global_target();
        Ok(target)
    }

    pub async fn get_outbound_info(&self, tag: &str) -> Result<OutBoundHandlerInfo, Error> {
        if let Some(info) = self.outbound_manager.read().await.get_outbound_info(tag) {
            Ok(info)
        } else {
            Err(Error::Config(anyhow!("not found")))
        }
    }

    pub async fn get_all_outbound_info(&self) -> Result<Vec<OutBoundHandlerInfo>, Error> {
        let lock = self.outbound_manager.read().await;
        let all_outbounds = lock.handlers();
        let mut result = Vec::new();
        for info in all_outbounds {
            result.push(info.clone());
        }
        Ok(result)
    }

    pub async fn get_all_outbounds_latency(&self) -> HashMap<String, u64> {
        self.outbound_manager
            .read()
            .await
            .get_all_outbounds_latency()
            .await
    }

    pub async fn cancel_all_sessions(&self) {
        self.dispatcher.cancel_all_sessions().await;
    }

    pub async fn measure_all_outbounds(
        &self,
    ) -> HashMap<String, tokio::sync::oneshot::Receiver<Measure>> {
        self.outbound_manager.read().await.measure_all_outbounds()
    }

    pub async fn measure_latency_for_outbounds(
        &self,
        tags: Vec<String>,
    ) -> HashMap<String, tokio::sync::oneshot::Receiver<Measure>> {
        self.outbound_manager
            .read()
            .await
            .update_measure_for_outbounds(tags)
    }

    pub async fn get_related_proxy_tags(
        &self,
        tags: Vec<String>,
    ) -> Result<HashSet<String>, Error> {
        let outbound_manager = self.outbound_manager.read().await;
        let mut result = HashSet::new();
        outbound_manager.collect_tags(&tags, &mut result);
        Ok(result)
    }

    // This function could block by an in-progress connection dialing.
    //
    // TODO Reload FakeDns. And perhaps the inbounds as long as the listening
    // addresses haven't changed.
    pub async fn reload(&self) -> Result<(), Error> {
        let config_path = if let Some(p) = self.config_path.as_ref() {
            p
        } else {
            return Err(Error::NoConfigFile);
        };
        info!("reloading from config file: {}", config_path);
        let mut config = config::from_file(config_path).map_err(Error::Config)?;
        #[cfg(not(feature = "no-tracing"))]
        app::logger::setup_logger(&config.log)?;
        self.router.write().await.reload(&mut config.router)?;
        self.dns_client.write().await.reload(&config.dns)?;
        self.outbound_manager
            .write()
            .await
            .reload(&config.outbounds, self.dns_client.clone())
            .await?;
        info!("reloaded from config file: {}", config_path);
        Ok(())
    }

    pub fn blocking_reload(&self) -> Result<(), Error> {
        let tx = self.reload_tx.clone();
        let (res_tx, res_rx) = sync_channel(0);
        if let Err(e) = tx.blocking_send(res_tx) {
            return Err(Error::AsyncChannelSend(e));
        }
        match res_rx.recv() {
            Ok(res) => res,
            Err(e) => Err(Error::SyncChannelRecv(e)),
        }
    }

    pub async fn shutdown(&self) -> bool {
        let tx = self.shutdown_tx.clone();
        if let Err(e) = tx.send(()).await {
            warn!("sending shutdown signal failed: {}", e);
            return false;
        }
        true
    }

    pub fn blocking_shutdown(&self) -> bool {
        let tx = self.shutdown_tx.clone();
        if let Err(e) = tx.blocking_send(()) {
            warn!("sending shutdown signal failed: {}", e);
            return false;
        }
        true
    }

    #[cfg(feature = "auto-reload")]
    pub(crate) fn new_watcher(&self) -> Result<(), Error> {
        let config_path = if let Some(p) = self.config_path.as_ref() {
            p
        } else {
            return Err(Error::NoConfigFile);
        };
        if self.auto_reload {
            trace!("starting new watcher for config file: {}", config_path);
            let rt_id = self.rt_id;
            let mut watcher: RecommendedWatcher =
                notify::recommended_watcher(move |res: NotifyResult<event::Event>| {
                    match res {
                        // FIXME Not sure what are the most appropriate events to
                        // filter on different platforms.
                        Ok(ev) => {
                            match ev.kind {
                                #[cfg(any(target_os = "macos", target_os = "ios"))]
                                event::EventKind::Modify(event::ModifyKind::Data(
                                    event::DataChange::Content,
                                )) => {
                                    info!("config file event matched: {:?}", ev);
                                    if let Err(e) = reload(rt_id) {
                                        warn!("reload config file failed: {}", e);
                                    }
                                }
                                #[cfg(any(target_os = "linux", target_os = "android"))]
                                event::EventKind::Access(event::AccessKind::Close(
                                    event::AccessMode::Write,
                                ))
                                | event::EventKind::Remove(event::RemoveKind::File) => {
                                    info!("config file event matched: {:?}", ev);
                                    if let Err(e) = reload(rt_id) {
                                        warn!("reload config file failed: {}", e);
                                    }
                                }
                                #[cfg(target_os = "windows")]
                                event::EventKind::Modify(event::ModifyKind::Data(
                                    event::DataChange::Any,
                                )) => {
                                    info!("config file event matched: {:?}", ev);
                                    if let Err(e) = reload(rt_id) {
                                        warn!("reload config file failed: {}", e);
                                    }
                                }
                                _ => {
                                    trace!("skip config file event: {:?}", ev);
                                }
                            }
                            // The config file could somehow be removed and re-created
                            // by an editor, in that case create a new watcher to watch
                            // the new file.
                            if let event::EventKind::Remove(event::RemoveKind::File) = ev.kind {
                                if let Some(m) = RUNTIME_MANAGER.lock().unwrap().get(&rt_id) {
                                    let _ = m.new_watcher();
                                }
                            }
                        }
                        Err(e) => {
                            error!("config file watch error: {:?}", e);
                        }
                    }
                })
                .map_err(Error::Watcher)?;
            watcher
                .watch(
                    std::path::Path::new(&config_path),
                    RecursiveMode::NonRecursive,
                )
                .map_err(Error::Watcher)?;
            info!("watching changes of file: {}", config_path);
            self.watcher.lock().unwrap().replace(watcher);
        }
        Ok(())
    }
}

pub type RuntimeId = u16;

lazy_static! {
    pub static ref RUNTIME_MANAGER: Mutex<HashMap<RuntimeId, Arc<RuntimeManager>>> =
        Mutex::new(HashMap::new());
}

pub fn reload(key: RuntimeId) -> Result<(), Error> {
    if let Some(m) = RUNTIME_MANAGER
        .lock()
        .map_err(|_| Error::RuntimeManager)?
        .get(&key)
    {
        return m.blocking_reload();
    }
    Err(Error::RuntimeManager)
}

pub fn shutdown(key: RuntimeId) -> bool {
    if let Some(m) = RUNTIME_MANAGER.lock().unwrap().get(&key) {
        return m.blocking_shutdown();
    }
    false
}

pub fn is_running(key: RuntimeId) -> bool {
    RUNTIME_MANAGER.lock().unwrap().contains_key(&key)
}

pub fn get_runtime_manager(key: RuntimeId) -> Option<Arc<RuntimeManager>> {
    RUNTIME_MANAGER.lock().unwrap().get(&key).cloned()
}

pub fn test_config(config_path: &str) -> Result<(), Error> {
    config::from_file(config_path)
        .map(|_| ())
        .map_err(Error::Config)
}

fn new_runtime(opt: &RuntimeOption) -> Result<tokio::runtime::Runtime, Error> {
    match opt {
        RuntimeOption::SingleThread => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(Error::Io),
        RuntimeOption::MultiThreadAuto(stack_size) => tokio::runtime::Builder::new_multi_thread()
            .thread_stack_size(*stack_size)
            .enable_all()
            .build()
            .map_err(Error::Io),
        RuntimeOption::MultiThread(worker_threads, stack_size) => {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(*worker_threads)
                .thread_stack_size(*stack_size)
                .enable_all()
                .build()
                .map_err(Error::Io)
        }
    }
}

#[derive(Debug)]
pub enum RuntimeOption {
    // Single-threaded runtime.
    SingleThread,
    // Multi-threaded runtime with thread stack size.
    MultiThreadAuto(usize),
    // Multi-threaded runtime with the number of worker threads and thread stack size.
    MultiThread(usize, usize),
}

#[derive(Debug)]
pub enum Config {
    File(String),
    Str(String),
    Internal(config::Config),
}

#[derive(Debug)]
pub struct StartOptions {
    // The path of the config.
    pub config: Config,
    // Enable auto reload, take effect only when "auto-reload" feature is enabled.
    #[cfg(feature = "auto-reload")]
    pub auto_reload: bool,
    // Tokio runtime options.
    pub runtime_opt: RuntimeOption,
}

pub fn start(
    rt_id: RuntimeId,
    opts: StartOptions,
    mut started_notify: Option<tokio::sync::oneshot::Sender<anyhow::Result<()>>>,
) -> Result<(), Error> {
    // #[cfg(debug_assertions)]
    // println!("start with options:\n{:#?}", opts);
    let (reload_tx, mut reload_rx) = mpsc::channel(1);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

    let config_path = match opts.config {
        Config::File(ref p) => Some(p.to_owned()),
        _ => None,
    };

    let mut config = match opts.config {
        Config::File(p) => config::from_file(&p).map_err(Error::Config)?,
        Config::Str(s) => config::from_string(&s).map_err(Error::Config)?,
        Config::Internal(c) => c,
    };
    #[cfg(not(feature = "no-tracing"))]
    app::logger::setup_logger(&config.log)?;

    let rt = new_runtime(&opts.runtime_opt)?;
    let _g = rt.enter();

    let mut tasks: Vec<Runner> = Vec::new();
    let mut runners = Vec::new();

    let dns_client = Arc::new(RwLock::new(
        DnsClient::new(&config.dns).map_err(Error::Config)?,
    ));
    let outbound_manager = Arc::new(RwLock::new(
        OutboundManager::new(&config.outbounds, dns_client.clone()).map_err(Error::Config)?,
    ));
    let router = Arc::new(RwLock::new(Router::new(
        &mut config.router,
        dns_client.clone(),
    )));
    #[cfg(feature = "stat")]
    let stat_manager = Arc::new(RwLock::new(StatManager::new()));
    #[cfg(feature = "stat")]
    runners.push(StatManager::cleanup_task(stat_manager.clone()));
    let dispatcher = Arc::new(Dispatcher::new(
        outbound_manager.clone(),
        router.clone(),
        dns_client.clone(),
        #[cfg(feature = "stat")]
        stat_manager.clone(),
    ));

    let dispatcher_weak = Arc::downgrade(&dispatcher);
    let dns_client_cloned = dns_client.clone();
    rt.block_on(async move {
        dns_client_cloned
            .write()
            .await
            .replace_dispatcher(dispatcher_weak);
    });

    let nat_manager = Arc::new(NatManager::new(dispatcher.clone()));
    let inbound_manager = InboundManager::new(&config.inbounds, dispatcher.clone(), nat_manager)
        .map_err(Error::Config)?;
    let mut inbound_net_runners = inbound_manager
        .get_network_runners()
        .map_err(Error::Config)?;
    runners.append(&mut inbound_net_runners);

    #[cfg(all(feature = "inbound-tun", any(target_os = "macos", target_os = "linux")))]
    let net_info = if inbound_manager.has_tun_listener() && inbound_manager.tun_auto() {
        sys::get_net_info().map_err(Error::Config)?
    } else {
        sys::NetInfo::default()
    };

    #[cfg(all(feature = "inbound-tun", any(target_os = "macos", target_os = "linux")))]
    {
        if let sys::NetInfo {
            default_interface: Some(iface),
            ..
        } = &net_info
        {
            use tracing::debug;

            let binds = if let Ok(v) = std::env::var("OUTBOUND_INTERFACE") {
                format!("{},{}", v, iface)
            } else {
                iface.clone()
            };
            debug!("OUTBOUND_INTERFACE: {}", binds);
            std::env::set_var("OUTBOUND_INTERFACE", binds);
        }
    }

    #[cfg(all(feature = "inbound-tun", target_os = "windows"))]
    {
        use tracing::debug;

        let binds = winsys::get_default_interface_ips();
        debug!("OUTBOUND_INTERFACE: {}", binds);
        std::env::set_var("OUTBOUND_INTERFACE", winsys::get_default_interface_ips());
    }

    #[cfg(all(
        feature = "inbound-tun",
        any(target_os = "linux", target_os = "windows", target_os = "macos")
    ))]
    let mut restore: Option<tproxy_config::TproxyState> = None;

    #[cfg(all(
        feature = "inbound-tun",
        any(
            target_os = "ios",
            target_os = "android",
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )
    ))]
    if let Ok((r, tun_info)) = inbound_manager.get_tun_runner() {
        runners.push(r);
        // for windows we using tproxy to setup the tun device
        #[cfg(target_os = "windows")]
        {
            // windows route process
            let mut tproxy_args = tproxy_config::TproxyArgs::new()
                .tun_name(&tun_info.tun_name)
                .tun_ip(tun_info.tun_ip)
                .tun_netmask(tun_info.tun_netmask)
                .tun_gateway(tun_info.tun_gateway)
                .tun_mtu(tun_info.tun_mtu);
            restore = Some(rt.block_on(tproxy_config::tproxy_setup(&tproxy_args))?);
        }
    }

    #[cfg(feature = "inbound-cat")]
    if let Ok(r) = inbound_manager.get_cat_runner() {
        runners.push(r);
    }

    #[cfg(all(feature = "inbound-tun", any(target_os = "macos", target_os = "linux")))]
    sys::post_tun_creation_setup(&net_info);

    let runtime_manager = RuntimeManager::new(
        #[cfg(feature = "auto-reload")]
        rt_id,
        config_path,
        #[cfg(feature = "auto-reload")]
        opts.auto_reload,
        reload_tx,
        shutdown_tx,
        router,
        dns_client,
        outbound_manager,
        #[cfg(feature = "stat")]
        stat_manager,
        dispatcher,
    );

    // Monitor config file changes.
    #[cfg(feature = "auto-reload")]
    {
        if let Err(e) = runtime_manager.new_watcher() {
            warn!("start config file watcher failed: {}", e);
        }
    }

    #[cfg(feature = "api")]
    {
        use std::net::SocketAddr;
        let listen_addr = if !option::API_LISTEN.is_empty() {
            Some(
                option::API_LISTEN
                    .parse::<SocketAddr>()
                    .map_err(|e| Error::Config(anyhow!("parse SocketAddr failed: {}", e)))?,
            )
        } else {
            None
        };
        if let Some(listen_addr) = listen_addr {
            let api_server = ApiServer::new(runtime_manager.clone());
            runners.push(api_server.serve(listen_addr));
        }
    }

    drop(config); // explicitly free the memory

    // Monitor reload signal.
    let rm = runtime_manager.clone();
    tasks.push(Box::pin(async move {
        loop {
            if let Some(res_tx) = reload_rx.recv().await {
                let res = rm.reload().await;
                if let Err(e) = res_tx.send(res) {
                    warn!("sending reload result failed: {}", e);
                }
            } else {
                warn!("receiving none reload signal");
            }
        }
    }));

    // The main task joining all runners.
    tasks.push(Box::pin(async move {
        futures::future::join_all(runners).await;
    }));

    // Monitor shutdown signal.
    tasks.push(Box::pin(async move {
        #[cfg(all(
            feature = "inbound-tun",
            any(target_os = "linux", target_os = "windows", target_os = "macos")
        ))]
        let _restore = restore.take();
        let _ = shutdown_rx.recv().await;
        trace!("shutdown signal received");
    }));

    // Monitor ctrl-c exit signal.
    #[cfg(feature = "ctrlc")]
    tasks.push(Box::pin(async move {
        let _ = tokio::signal::ctrl_c().await;
    }));

    RUNTIME_MANAGER
        .lock()
        .map_err(|_| Error::RuntimeManager)?
        .insert(rt_id, runtime_manager);

    // #[cfg(not(target_os = "windows"))]
    // {
    info!("added runtime {}", &rt_id);
    if let Some(started_notify) = started_notify {
        let _ = started_notify.send(Ok(()));
    }
    rt.block_on(futures::future::select_all(tasks));

    trace!("shutdown rutime: {}", &rt_id);

    #[cfg(all(feature = "inbound-tun", any(target_os = "macos", target_os = "linux")))]
    sys::post_tun_completion_setup(&net_info);

    drop(inbound_manager);

    RUNTIME_MANAGER
        .lock()
        .map_err(|_| Error::RuntimeManager)?
        .remove(&rt_id);

    rt.shutdown_background();

    trace!("removed runtime {}", &rt_id);
    // }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_restart() {
        let conf = r#"
[General]
loglevel = trace
dns-server = 1.1.1.1
socks-interface = 127.0.0.1
socks-port = 1080
# tun = auto

[Proxy]
Direct = direct
"#;

        for _i in 1..3 {
            thread::spawn(move || {
                let opts = StartOptions {
                    config: Config::Str(conf.to_string()),
                    #[cfg(feature = "auto-reload")]
                    auto_reload: false,
                    runtime_opt: RuntimeOption::SingleThread,
                };
                start(0, opts, None).unwrap();
            });
            thread::sleep(std::time::Duration::from_secs(2));
            assert!(shutdown(0));
            loop {
                thread::sleep(std::time::Duration::from_secs(1));
                if !is_running(0) {
                    break;
                }
            }
        }
    }
}
