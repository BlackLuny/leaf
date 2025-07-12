use std::{
    collections::{hash_map, HashMap, HashSet, VecDeque},
    convert::From,
    ops::Deref,
};

use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures::{
    future::{join_all, select_all, AbortHandle},
    StreamExt,
};
use protobuf::Message;
use tracing::{debug, trace};

#[cfg(feature = "outbound-select")]
use tokio::sync::RwLock;

#[cfg(feature = "outbound-chain")]
use crate::proxy::chain;
#[cfg(feature = "outbound-failover")]
use crate::proxy::failover;
#[cfg(feature = "outbound-static")]
use crate::proxy::r#static;
#[cfg(feature = "outbound-select")]
use crate::proxy::select;
#[cfg(feature = "outbound-tryall")]
use crate::proxy::tryall;

#[cfg(feature = "outbound-amux")]
use crate::proxy::amux;
#[cfg(feature = "outbound-direct")]
use crate::proxy::direct;
#[cfg(feature = "outbound-drop")]
use crate::proxy::drop;
#[cfg(feature = "outbound-obfs")]
use crate::proxy::obfs;
#[cfg(feature = "outbound-private-tun")]
use crate::proxy::private_tun;
#[cfg(feature = "outbound-quic")]
use crate::proxy::quic;
#[cfg(feature = "outbound-redirect")]
use crate::proxy::redirect;
#[cfg(feature = "outbound-shadowsocks")]
use crate::proxy::shadowsocks;
#[cfg(feature = "outbound-socks")]
use crate::proxy::socks;
#[cfg(feature = "outbound-tls")]
use crate::proxy::tls;
#[cfg(feature = "outbound-trojan")]
use crate::proxy::trojan;
#[cfg(feature = "outbound-vmess")]
use crate::proxy::vmess;
#[cfg(feature = "outbound-ws")]
use crate::proxy::ws;

use crate::{
    app::{dns_client::DnsClient, SyncDnsClient},
    config::{self, Outbound},
    proxy::{failover::health_check, outbound::HandlerBuilder, *},
};

#[cfg(feature = "outbound-select")]
use super::selector::OutboundSelector;

use crate::proxy::failover::Measure;

#[derive(Clone)]
pub struct OutBoundHandlerInfo {
    handler: AnyOutboundHandler,
    protocol: String,
    settings: Arc<Vec<u8>>,
    sub_handlers: Arc<Vec<String>>,
    tag: String,
    measures: Arc<RwLock<VecDeque<Measure>>>,
}

impl OutBoundHandlerInfo {
    pub fn new(
        tag: String,
        handler: AnyOutboundHandler,
        protocol: String,
        settings: Vec<u8>,
        sub_handlers: Vec<String>,
    ) -> Self {
        Self {
            tag,
            handler,
            protocol,
            settings: Arc::new(settings),
            sub_handlers: Arc::new(sub_handlers),
            measures: Arc::new(RwLock::new(VecDeque::new())),
        }
    }
    pub fn handler(&self) -> &AnyOutboundHandler {
        &self.handler
    }
    pub fn protocol(&self) -> &str {
        &self.protocol
    }
    pub fn settings(&self) -> &Vec<u8> {
        &self.settings
    }
    pub fn tag(&self) -> &str {
        &self.tag
    }
    pub fn sub_handlers(&self) -> &Vec<String> {
        &self.sub_handlers
    }
    pub async fn append_measure(&self, measure: Measure) {
        let mut lock = self.measures.write().await;
        if lock.len() >= 10 {
            lock.pop_front();
        }
        lock.push_back(measure);
    }
    pub async fn get_latest_measure(&self) -> Option<Measure> {
        self.measures.read().await.back().cloned()
    }
}

impl Deref for OutBoundHandlerInfo {
    type Target = AnyOutboundHandler;
    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

pub struct OutboundManager {
    handlers: HashMap<String, OutBoundHandlerInfo>,
    #[cfg(feature = "plugin")]
    external_handlers: super::plugin::ExternalHandlers,
    #[cfg(feature = "outbound-select")]
    selectors: Arc<super::Selectors>,
    default_handler: Option<String>,
    abort_handles: Vec<AbortHandle>,
    dns_client: SyncDnsClient,
}

struct HandlerCacheEntry<'a> {
    tag: &'a str,
    handler: AnyOutboundHandler,
    protocol: &'a str,
    settings: &'a Vec<u8>,
}

impl OutboundManager {
    #[allow(clippy::type_complexity)]
    fn load_handlers(
        outbounds: &[Outbound],
        dns_client: SyncDnsClient,
        handlers: &mut HashMap<String, OutBoundHandlerInfo>,
        #[cfg(feature = "plugin")] external_handlers: &mut super::plugin::ExternalHandlers,
        default_handler: &mut Option<String>,
        abort_handles: &mut Vec<AbortHandle>,
    ) -> Result<()> {
        // If there are multiple outbounds with the same setting, we would want
        // a shared one to reduce memory usage. This vector is used as a cache for
        // unseen outbounds so we can reuse them later.
        let mut cached_handlers: Vec<HandlerCacheEntry> = Vec::new();

        'loop1: for outbound in outbounds.iter() {
            let tag = String::from(&outbound.tag);
            if handlers.contains_key(&tag) {
                continue;
            }
            if default_handler.is_none() {
                default_handler.replace(String::from(&outbound.tag));
                debug!("default handler [{}]", &outbound.tag);
            }

            // Check whether an identical one already exist.
            for e in cached_handlers.iter() {
                if e.protocol == outbound.protocol && e.settings == &outbound.settings {
                    trace!("add handler [{}] cloned from [{}]", &tag, &e.tag);
                    handlers.insert(
                        tag.clone(),
                        OutBoundHandlerInfo::new(
                            tag.clone(),
                            e.handler.clone(),
                            e.protocol.to_owned(),
                            e.settings.clone(),
                            vec![],
                        ),
                    );
                    continue 'loop1;
                }
            }

            let h: AnyOutboundHandler = match outbound.protocol.as_str() {
                #[cfg(feature = "outbound-direct")]
                "direct" => HandlerBuilder::default()
                    .tag(tag.clone())
                    .color(colored::Color::Green)
                    .stream_handler(Box::new(direct::StreamHandler))
                    .datagram_handler(Box::new(direct::DatagramHandler))
                    .build(),
                #[cfg(feature = "outbound-drop")]
                "drop" => HandlerBuilder::default()
                    .tag(tag.clone())
                    .color(colored::Color::Red)
                    .stream_handler(Box::new(drop::StreamHandler))
                    .datagram_handler(Box::new(drop::DatagramHandler))
                    .build(),
                #[cfg(feature = "outbound-redirect")]
                "redirect" => {
                    let settings =
                        config::RedirectOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(redirect::StreamHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                    });
                    let datagram = Box::new(redirect::DatagramHandler {
                        address: settings.address,
                        port: settings.port as u16,
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                #[cfg(feature = "outbound-socks")]
                "socks" => {
                    let settings =
                        config::SocksOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(socks::outbound::StreamHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                        username: settings.username.clone(),
                        password: settings.password.clone(),
                    });
                    let datagram = Box::new(socks::outbound::DatagramHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                        dns_client: dns_client.clone(),
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                #[cfg(feature = "outbound-shadowsocks")]
                "shadowsocks" => {
                    let settings =
                        config::ShadowsocksOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(shadowsocks::outbound::StreamHandler::new(
                        settings.address.clone(),
                        settings.port as u16,
                        settings.method.clone(),
                        settings.password.clone(),
                        settings.prefix.as_ref().cloned(),
                    )?);
                    let datagram = Box::new(shadowsocks::outbound::DatagramHandler {
                        address: settings.address,
                        port: settings.port as u16,
                        cipher: settings.method,
                        password: settings.password,
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                #[cfg(feature = "outbound-obfs")]
                "obfs" => {
                    let settings =
                        config::ObfsOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = match &*settings.method {
                        "http" => Box::new(obfs::HttpObfsStreamHandler::new(
                            settings.path.as_bytes(),
                            settings.host.as_bytes(),
                        )) as _,
                        "tls" => {
                            Box::new(obfs::TlsObfsStreamHandler::new(settings.host.as_bytes())) as _
                        }
                        method => {
                            return Err(anyhow!(
                                "invalid [{}] outbound settings: unknown obfs method {}",
                                &tag,
                                method
                            ))
                        }
                    };
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .build()
                }
                #[cfg(feature = "outbound-trojan")]
                "trojan" => {
                    let settings =
                        config::TrojanOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(trojan::outbound::StreamHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                        password: settings.password.clone(),
                    });
                    let datagram = Box::new(trojan::outbound::DatagramHandler {
                        address: settings.address,
                        port: settings.port as u16,
                        password: settings.password,
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                #[cfg(feature = "outbound-vmess")]
                "vmess" => {
                    let settings =
                        config::VMessOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(vmess::outbound::StreamHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                        uuid: settings.uuid.clone(),
                        security: settings.security.clone(),
                    });
                    let datagram = Box::new(vmess::outbound::DatagramHandler {
                        address: settings.address.clone(),
                        port: settings.port as u16,
                        uuid: settings.uuid.clone(),
                        security: settings.security.clone(),
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                #[cfg(feature = "outbound-tls")]
                "tls" => {
                    let settings =
                        config::TlsOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let certificate = if settings.certificate.is_empty() {
                        None
                    } else {
                        Some(settings.certificate.clone())
                    };
                    let stream = Box::new(tls::outbound::StreamHandler::new(
                        settings.server_name.clone(),
                        settings.alpn.clone(),
                        certificate,
                        settings.insecure,
                    )?);
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .build()
                }
                #[cfg(feature = "outbound-ws")]
                "ws" => {
                    let settings =
                        config::WebSocketOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let stream = Box::new(ws::outbound::StreamHandler {
                        path: settings.path.clone(),
                        headers: settings.headers.clone(),
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .build()
                }
                #[cfg(feature = "outbound-quic")]
                "quic" => {
                    let settings =
                        config::QuicOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;
                    let server_name = if settings.server_name.is_empty() {
                        None
                    } else {
                        Some(settings.server_name.clone())
                    };
                    let certificate = if settings.certificate.is_empty() {
                        None
                    } else {
                        Some(settings.certificate.clone())
                    };
                    let stream = Box::new(quic::outbound::StreamHandler::new(
                        settings.address.clone(),
                        settings.port as u16,
                        server_name,
                        settings.alpn.clone(),
                        certificate,
                        dns_client.clone(),
                    ));
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .build()
                }
                #[cfg(feature = "outbound-private-tun")]
                "private-tun" => {
                    use std::{net::IpAddr, sync::Arc};

                    use ::private_tun::snell_impl_ver::{
                        client_zfc::run_client_with_config_and_name,
                        udp_intf::create_ringbuf_channel,
                    };

                    let settings =
                        config::PrivateTunOutboundSettings::parse_from_bytes(&outbound.settings)
                            .map_err(|e| anyhow!("invalid [{}] outbound settings: {}", &tag, e))?;

                    // 解析存储的ClientConfig JSON
                    let client_config: ::private_tun::snell_impl_ver::config::ClientConfig =
                        serde_json::from_str(&settings.client_config_json)
                            .map_err(|e| anyhow!("invalid private-tun client config: {}", e))?;

                    let cancel_token = tokio_util::sync::CancellationToken::new();
                    let cancel_token_clone = cancel_token.clone();
                    let (inbound_tx, inbound_rx) = create_ringbuf_channel(13);
                    let config_name = tag.clone();
                    struct DnsClientWrapper(SyncDnsClient);
                    #[async_trait::async_trait]
                    impl ::private_tun::dns_cache::DnsResolver for DnsClientWrapper {
                        async fn resolve_dns(&self, host: &str, port: u16) -> Result<Vec<IpAddr>> {
                            self.0.read().await.direct_lookup(&host.to_string()).await
                        }
                    }
                    let dns_resolver = Arc::new(DnsClientWrapper(dns_client.clone()))
                        as Arc<dyn ::private_tun::dns_cache::DnsResolver>;
                    let notifier = Arc::new(tokio::sync::Notify::new());
                    let notifier_clone = notifier.clone();
                    tokio::spawn(async move {
                        use ::private_tun::snell_impl_ver::client_run::init_ring_provider;
                        use socket2::Socket;

                        use crate::session::SocksAddr;
                        let _ = init_ring_provider();
                        notifier_clone.notified().await;
                        let _h = run_client_with_config_and_name(
                            client_config,
                            inbound_rx,
                            &config_name,
                            Some(cancel_token),
                            true,
                            Some(Arc::new(Box::new(move |socket: &Socket, target_addr| {
                                let r = bind_socket(socket, target_addr);
                                debug!("hammer bind socket to {}: {:?}", target_addr, r);
                            }))),
                            Some(dns_resolver),
                        )
                        .await;
                    });
                    let client = Arc::new(private_tun::outbound::Client::new(
                        tokio::sync::Mutex::new(inbound_tx),
                        cancel_token_clone,
                        notifier,
                    ));

                    // Create stream handler
                    let stream = Box::new(private_tun::outbound::Handler {
                        tag: tag.clone(),
                        color: colored::Color::Yellow, // 默认颜色
                        client: client.clone(),
                    });

                    let datagram = Box::new(private_tun::outbound::DatagramHandler {
                        tag: tag.clone(),
                        color: colored::Color::Yellow, // 默认颜色
                        bind_addr: "127.0.0.1:0".parse().unwrap(), // 默认绑定地址
                        client: client.clone(),
                    });
                    HandlerBuilder::default()
                        .tag(tag.clone())
                        .stream_handler(stream)
                        .datagram_handler(datagram)
                        .build()
                }
                _ => continue,
            };
            cached_handlers.push(HandlerCacheEntry {
                tag: &outbound.tag,
                handler: h.clone(),
                protocol: &outbound.protocol,
                settings: &outbound.settings,
            });
            trace!("add handler [{}]", &tag);
            handlers.insert(
                tag.clone(),
                OutBoundHandlerInfo::new(
                    tag.clone(),
                    h.clone(),
                    outbound.protocol.clone(),
                    outbound.settings.clone(),
                    vec![],
                ),
            );
        }

        drop(cached_handlers);

        // FIXME a better way to find outbound deps?
        for _i in 0..8 {
            'outbounds: for outbound in outbounds.iter() {
                let tag = String::from(&outbound.tag);
                if handlers.contains_key(&tag) {
                    continue;
                }
                match outbound.protocol.as_str() {
                    #[cfg(feature = "outbound-tryall")]
                    "tryall" => {
                        let settings =
                            config::TryAllOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.clone());
                            } else {
                                continue 'outbounds;
                            }
                        }
                        if actors.is_empty() {
                            continue;
                        }
                        let stream = Box::new(tryall::StreamHandler {
                            actors: actors.iter().map(|x| x.handler().clone()).collect(),
                            delay_base: settings.delay_base,
                            dns_client: dns_client.clone(),
                        });
                        let datagram = Box::new(tryall::DatagramHandler {
                            actors: actors.iter().map(|x| x.handler().clone()).collect(),
                            delay_base: settings.delay_base,
                            dns_client: dns_client.clone(),
                        });
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(stream)
                            .datagram_handler(datagram)
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "tryall".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    #[cfg(feature = "outbound-static")]
                    "static" => {
                        let settings =
                            config::StaticOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.clone());
                            } else {
                                continue 'outbounds;
                            }
                        }
                        if actors.is_empty() {
                            continue;
                        }
                        let stream = Box::new(r#static::StreamHandler::new(
                            actors.iter().map(|x| x.handler().clone()).collect(),
                            &settings.method,
                        )?);
                        let datagram = Box::new(r#static::DatagramHandler::new(
                            actors.iter().map(|x| x.handler().clone()).collect(),
                            &settings.method,
                        )?);
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(stream)
                            .datagram_handler(datagram)
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "static".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    #[cfg(feature = "outbound-failover")]
                    "failover" => {
                        let settings =
                            config::FailOverOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.clone());
                            } else {
                                continue 'outbounds;
                            }
                        }
                        if actors.is_empty() {
                            continue;
                        }
                        let last_resort =
                            if let Some(last_resort_tag) = settings.last_resort.as_ref() {
                                handlers.get(last_resort_tag).map(|x| x.handler().clone())
                            } else {
                                None
                            };
                        let (stream, mut stream_abort_handles) = failover::StreamHandler::new(
                            actors.iter().map(|x| x.handler().clone()).collect(),
                            settings.fail_timeout,
                            settings.health_check,
                            settings.check_interval,
                            settings.failover,
                            settings.fallback_cache,
                            settings.cache_size as usize,
                            settings.cache_timeout as u64,
                            last_resort.clone(),
                            settings.health_check_timeout,
                            settings.health_check_delay,
                            settings.health_check_active,
                            settings.health_check_prefers.clone(),
                            settings.health_check_on_start,
                            settings.health_check_wait,
                            settings.health_check_attempts,
                            settings.health_check_success_percentage,
                            dns_client.clone(),
                        );
                        let (datagram, mut datagram_abort_handles) = failover::DatagramHandler::new(
                            actors.iter().map(|x| x.handler().clone()).collect(),
                            settings.fail_timeout,
                            settings.health_check,
                            settings.check_interval,
                            settings.failover,
                            last_resort,
                            settings.health_check_timeout,
                            settings.health_check_delay,
                            settings.health_check_active,
                            settings.health_check_prefers,
                            settings.health_check_on_start,
                            settings.health_check_wait,
                            settings.health_check_attempts,
                            settings.health_check_success_percentage,
                            dns_client.clone(),
                        );
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(Box::new(stream))
                            .datagram_handler(Box::new(datagram))
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "failover".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        abort_handles.append(&mut stream_abort_handles);
                        abort_handles.append(&mut datagram_abort_handles);
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    #[cfg(feature = "outbound-amux")]
                    "amux" => {
                        let settings =
                            config::AMuxOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.clone());
                            } else {
                                continue 'outbounds;
                            }
                        }
                        let (stream, mut stream_abort_handles) = amux::outbound::StreamHandler::new(
                            settings.address.clone(),
                            settings.port as u16,
                            actors.iter().map(|x| x.handler().clone()).collect(),
                            settings.max_accepts as usize,
                            settings.concurrency as usize,
                            settings.max_recv_bytes as usize,
                            settings.max_lifetime,
                            dns_client.clone(),
                        );
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(Box::new(stream))
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "amux".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        abort_handles.append(&mut stream_abort_handles);
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    #[cfg(feature = "outbound-chain")]
                    "chain" => {
                        let settings =
                            config::ChainOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.clone());
                            } else {
                                continue 'outbounds;
                            }
                        }
                        if actors.is_empty() {
                            continue;
                        }
                        let stream = Box::new(chain::outbound::StreamHandler {
                            actors: actors.iter().map(|x| x.handler().clone()).collect(),
                        });
                        let datagram = Box::new(chain::outbound::DatagramHandler {
                            actors: actors.iter().map(|x| x.handler().clone()).collect(),
                        });
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(stream)
                            .datagram_handler(datagram)
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "chain".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    #[cfg(feature = "plugin")]
                    "plugin" => {
                        let settings =
                            config::PluginOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        unsafe {
                            external_handlers
                                .new_handler(settings.path, &tag, &settings.args)
                                .unwrap()
                        };
                        let stream = Box::new(super::plugin::ExternalOutboundStreamHandlerProxy(
                            external_handlers.get_stream_handler(&tag).unwrap(),
                        ));
                        let datagram =
                            Box::new(super::plugin::ExternalOutboundDatagramHandlerProxy(
                                external_handlers.get_datagram_handler(&tag).unwrap(),
                            ));
                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(stream)
                            .datagram_handler(datagram)
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "plugin".to_string(),
                                outbound.settings.clone(),
                                vec![],
                            ),
                        );
                        trace!("added handler [{}]", &tag,);
                    }
                    _ => continue,
                }
            }
        }

        Ok(())
    }

    #[allow(unused_variables)]
    fn load_selectors(
        outbounds: &[Outbound],
        handlers: &mut HashMap<String, OutBoundHandlerInfo>,
        #[cfg(feature = "plugin")] external_handlers: &mut super::plugin::ExternalHandlers,

        #[cfg(feature = "outbound-select")] selectors: &mut super::Selectors,
    ) -> Result<()> {
        // FIXME a better way to find outbound deps?
        for _i in 0..8 {
            #[allow(unused_labels)]
            'outbounds: for outbound in outbounds.iter() {
                let tag = String::from(&outbound.tag);
                if handlers.contains_key(&tag) {
                    continue;
                }
                #[cfg(feature = "outbound-select")]
                {
                    if selectors.contains_key(&tag) {
                        continue;
                    }
                }
                #[allow(clippy::single_match)]
                match outbound.protocol.as_str() {
                    #[cfg(feature = "outbound-select")]
                    "select" => {
                        let settings =
                            config::SelectOutboundSettings::parse_from_bytes(&outbound.settings)
                                .map_err(|e| {
                                    anyhow!("invalid [{}] outbound settings: {}", &tag, e)
                                })?;
                        let mut actors = Vec::new();
                        for actor in settings.actors.iter() {
                            if let Some(a) = handlers.get(actor) {
                                actors.push(a.handler.clone());
                            } else {
                                tracing::info!("select: {tag} actor not found: {}", actor);
                                continue 'outbounds;
                            }
                        }
                        if actors.is_empty() {
                            continue;
                        }

                        let actors_tags: Vec<String> =
                            actors.iter().map(|x| x.tag().to_owned()).collect();

                        use std::sync::atomic::AtomicUsize;
                        let selected = Arc::new(AtomicUsize::new(0));

                        let mut selector =
                            OutboundSelector::new(tag.clone(), actors_tags, selected.clone());
                        if let Ok(Some(selected)) = super::selector::get_selected_from_cache(&tag) {
                            // FIXME handle error
                            let _ = selector.set_selected(&selected);
                        } else {
                            let _ = selector.set_selected(&settings.actors[0]);
                        }
                        let selector = Arc::new(RwLock::new(selector));

                        let stream = Box::new(select::StreamHandler {
                            actors: actors.clone(),
                            selected: selected.clone(),
                        });
                        let datagram = Box::new(select::DatagramHandler {
                            actors: actors.clone(),
                            selected: selected.clone(),
                        });

                        #[cfg(feature = "outbound-select")]
                        {
                            selectors.insert(tag.clone(), selector);
                        }

                        let handler = HandlerBuilder::default()
                            .tag(tag.clone())
                            .stream_handler(stream)
                            .datagram_handler(datagram)
                            .build();
                        handlers.insert(
                            tag.clone(),
                            OutBoundHandlerInfo::new(
                                tag.clone(),
                                handler,
                                "select".to_string(),
                                outbound.settings.clone(),
                                actors.iter().map(|x| x.tag().to_string()).collect(),
                            ),
                        );
                        trace!(
                            "added handler [{}] with actors: {}",
                            &tag,
                            settings.actors.join(",")
                        );
                    }
                    _ => continue,
                }
            }
        }

        Ok(())
    }

    // TODO make this non-async?
    pub async fn reload(
        &mut self,
        outbounds: &[Outbound],
        dns_client: SyncDnsClient,
    ) -> Result<()> {
        // Save outound select states.
        #[cfg(feature = "outbound-select")]
        let mut selected_outbounds = HashMap::new();
        #[cfg(feature = "outbound-select")]
        {
            for (k, v) in self.selectors.iter() {
                selected_outbounds.insert(k.to_owned(), v.read().await.get_selected_tag());
            }
        }

        // Load new outbounds.
        let mut handlers: HashMap<String, OutBoundHandlerInfo> = HashMap::new();

        #[cfg(feature = "plugin")]
        let mut external_handlers = super::plugin::ExternalHandlers::new();
        let mut default_handler: Option<String> = None;
        let mut abort_handles: Vec<AbortHandle> = Vec::new();

        #[cfg(feature = "outbound-select")]
        let mut selectors: super::Selectors = HashMap::new();

        for _i in 0..4 {
            Self::load_handlers(
                outbounds,
                dns_client.clone(),
                &mut handlers,
                #[cfg(feature = "plugin")]
                &mut external_handlers,
                &mut default_handler,
                &mut abort_handles,
            )?;
            Self::load_selectors(
                outbounds,
                &mut handlers,
                #[cfg(feature = "plugin")]
                &mut external_handlers,
                #[cfg(feature = "outbound-select")]
                &mut selectors,
            )?;
        }

        // Restore outbound select states.
        #[cfg(feature = "outbound-select")]
        {
            for (k, v) in selected_outbounds.iter() {
                for (k2, v2) in selectors.iter_mut() {
                    if k == k2 {
                        let _ = v2.write().await.set_selected(v);
                    }
                }
            }
        }

        // Abort spawned tasks inside handlers.
        for abort_handle in self.abort_handles.iter() {
            abort_handle.abort();
        }

        self.handlers = handlers;

        #[cfg(feature = "plugin")]
        {
            self.external_handlers = external_handlers;
        }
        #[cfg(feature = "outbound-select")]
        {
            self.selectors = Arc::new(selectors);
        }

        self.default_handler = default_handler;
        self.abort_handles = abort_handles;
        self.dns_client = dns_client;
        Ok(())
    }

    pub fn new(outbounds: &[Outbound], dns_client: SyncDnsClient) -> Result<Self> {
        let mut handlers: HashMap<String, OutBoundHandlerInfo> = HashMap::new();
        #[cfg(feature = "plugin")]
        let mut external_handlers = super::plugin::ExternalHandlers::new();
        let mut default_handler: Option<String> = None;
        let mut abort_handles: Vec<AbortHandle> = Vec::new();
        #[cfg(feature = "outbound-select")]
        let mut selectors: super::Selectors = HashMap::new();
        for _i in 0..4 {
            Self::load_handlers(
                outbounds,
                dns_client.clone(),
                &mut handlers,
                #[cfg(feature = "plugin")]
                &mut external_handlers,
                &mut default_handler,
                &mut abort_handles,
            )?;
            Self::load_selectors(
                outbounds,
                &mut handlers,
                #[cfg(feature = "plugin")]
                &mut external_handlers,
                #[cfg(feature = "outbound-select")]
                &mut selectors,
            )?;
        }
        Ok(OutboundManager {
            handlers,
            #[cfg(feature = "plugin")]
            external_handlers,

            #[cfg(feature = "outbound-select")]
            selectors: Arc::new(selectors),
            default_handler,
            abort_handles,
            dns_client,
        })
    }

    pub fn add(
        &mut self,
        tag: String,
        handler: AnyOutboundHandler,
        protocol: String,
        settings: Vec<u8>,
        sub_handlers: Vec<String>,
    ) {
        self.handlers.insert(
            tag.clone(),
            OutBoundHandlerInfo::new(tag.clone(), handler, protocol, settings, sub_handlers),
        );
    }

    pub fn get(&self, tag: &str) -> Option<AnyOutboundHandler> {
        self.handlers.get(tag).map(|x| x.handler.clone())
    }

    pub fn get_outbound_info(&self, tag: &str) -> Option<OutBoundHandlerInfo> {
        self.handlers.get(tag).map(|x| x.clone())
    }

    pub fn default_handler(&self) -> Option<String> {
        self.default_handler.clone()
    }

    pub fn handlers(&self) -> Handlers {
        Handlers {
            inner: self.handlers.values(),
        }
    }

    #[cfg(feature = "outbound-select")]
    pub fn get_selector(&self, tag: &str) -> Option<Arc<RwLock<OutboundSelector>>> {
        self.selectors.get(tag).map(Clone::clone)
    }

    #[cfg(feature = "outbound-select")]
    pub fn get_selectable_outbounds(&self) -> Vec<(String)> {
        self.selectors.keys().map(|x| x.to_owned()).collect()
    }

    pub fn collect_tags(&self, targets: &Vec<String>, result: &mut HashSet<String>) {
        let mut working_set = targets
            .iter()
            .map(|x| (x.clone(), true))
            .collect::<VecDeque<_>>();
        while let Some((tag, should_check_sub_handlers)) = working_set.pop_front() {
            if let Some(handler) = self.handlers.get(&tag) {
                result.insert(tag.clone());

                // sub handlers
                if should_check_sub_handlers && handler.sub_handlers().len() > 0 {
                    for sub_tag in handler.sub_handlers() {
                        if !result.contains(sub_tag) {
                            working_set.push_back((sub_tag.clone(), true));
                        }
                    }
                }

                // parent handlers
                for handler in self.handlers().into_iter() {
                    if handler.sub_handlers().contains(&tag) && !result.contains(handler.tag()) {
                        working_set.push_back((handler.tag().to_string(), false));
                    }
                }
            }
        }
    }

    pub fn update_measure_for_outbounds(
        &self,
        outbounds_tags: Vec<String>,
    ) -> HashMap<String, tokio::sync::oneshot::Receiver<Measure>> {
        // get all measures for outbounds_tags
        let mut targets_for_test = HashSet::new();
        self.collect_tags(&outbounds_tags, &mut targets_for_test);

        tracing::info!("targets_for_test: {:?}", targets_for_test);

        let mut all_broadcast_sender = HashMap::new();
        for tag in targets_for_test.iter() {
            let (tx, rx) = tokio::sync::broadcast::channel::<Measure>(1);
            all_broadcast_sender.insert(tag.clone(), (tx, rx));
        }

        let mut task_handles = HashMap::new();

        let measures_stream = targets_for_test
            .into_iter()
            .map(|tag| {
                let h = self.handlers.get(&tag).unwrap().clone();
                let dns_client = self.dns_client.clone();
                let delay = 50;
                let health_check_timeout = 2000;
                let health_check_attempts = 1;
                let health_check_success_percentage = 100;
                let result_measure = h.measures.clone();
                let (tx, mut rx) = tokio::sync::oneshot::channel::<Measure>();

                let sbu_tasks = h
                    .sub_handlers()
                    .iter()
                    .filter_map(|sub_tag| {
                        if let Some((tx, _rx)) = all_broadcast_sender.get(sub_tag) {
                            let tx = tx.clone();
                            let sub_tag = sub_tag.clone();
                            Some(Box::pin(async move {
                                let mut rx = tx.subscribe();
                                let result = rx.recv().await.unwrap();
                                (sub_tag, result)
                            }))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                task_handles.insert(tag.clone(), rx);
                let broad_sender = all_broadcast_sender.get(&tag).map(|x| x.0.clone());
                #[cfg(feature = "outbound-select")]
                let selectors = self.selectors.clone();

                let all_sub_handlers = h
                    .sub_handlers()
                    .iter()
                    .filter_map(|x| self.get_outbound_info(x))
                    .collect::<Vec<_>>();
                Box::pin(async move {
                    let final_result = if sbu_tasks.len() > 0 {
                        // 等待所有子任务完成
                        let _sub_completed = join_all(sbu_tasks).await;
                        let mut sub_results = Vec::new();
                        // 获取所有子任务的最新测量结果
                        for sub_handler in all_sub_handlers {
                            let measure = sub_handler.get_latest_measure().await;
                            if let Some(measure) = measure {
                                sub_results.push((sub_handler.tag().to_string(), measure));
                            }
                        }
                        get_final_result_for_group(
                            sub_results,
                            &h,
                            #[cfg(feature = "outbound-select")]
                            selectors,
                        )
                        .await
                    } else {
                        health_check(
                            crate::session::Network::Tcp,
                            0,
                            tag,
                            h.handler().clone(),
                            dns_client,
                            delay,
                            health_check_timeout,
                            health_check_attempts,
                            health_check_success_percentage,
                        )
                        .await
                    };
                    result_measure.write().await.push_back(final_result.clone());
                    // send to broadcaster
                    if let Some(broad_sender) = broad_sender {
                        let _ = broad_sender.send(final_result.clone());
                    }
                    // send to oneshot channel
                    let _ = tx.send(final_result);
                })
            })
            .collect::<Vec<_>>();
        tokio::spawn(async move {
            join_all(measures_stream).await;
        });
        task_handles
    }

    pub fn measure_all_outbounds(
        &self,
    ) -> HashMap<String, tokio::sync::oneshot::Receiver<Measure>> {
        let all_outbounds_tags = self.handlers.keys().map(|x| x.to_owned()).collect();
        self.update_measure_for_outbounds(all_outbounds_tags)
    }
}

async fn get_final_result_for_group(
    sub_results: Vec<(String, Measure)>,
    handler_info: &OutBoundHandlerInfo,
    #[cfg(feature = "outbound-select")] selectors: Arc<super::Selectors>,
) -> Measure {
    match handler_info.protocol() {
        #[cfg(feature = "outbound-select")]
        "select" => {
            let selected = selectors
                .get(handler_info.tag())
                .unwrap()
                .read()
                .await
                .get_selected_tag();
            let selected_result = sub_results.iter().find(|x| x.0 == selected);
            if let Some(selected_result) = selected_result {
                selected_result.1.clone()
            } else {
                Measure::new(0, u128::MAX, handler_info.tag().to_string())
            }
        }
        "failover" => {
            let first_of_ok = sub_results.iter().find(|x| x.1.is_ok());
            if let Some(first_of_ok) = first_of_ok {
                first_of_ok.1.clone()
            } else {
                Measure::new(0, u128::MAX, handler_info.tag().to_string())
            }
        }
        _ => {
            // return smallest rtt
            let mut sub_results = sub_results
                .into_iter()
                .map(|x| (x.1.rtt(), x.1))
                .collect::<Vec<_>>();
            sub_results.sort_by(|a, b| a.0.cmp(&b.0));
            if sub_results.len() > 0 {
                sub_results[0].1.clone()
            } else {
                Measure::new(0, u128::MAX, handler_info.tag().to_string())
            }
        }
    }
}

pub struct Handlers<'a> {
    inner: hash_map::Values<'a, String, OutBoundHandlerInfo>,
}

impl<'a> Iterator for Handlers<'a> {
    type Item = &'a OutBoundHandlerInfo;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
