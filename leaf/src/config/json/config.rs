use std::collections::HashMap;
use std::path::Path;

use anyhow::anyhow;
use anyhow::Result;
use protobuf::Message;
use serde_derive::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::config::{external_rule, internal};

#[derive(Serialize, Deserialize, Debug)]
pub struct Dns {
    pub servers: Option<Vec<String>>,
    pub hosts: Option<HashMap<String, Vec<String>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Log {
    pub level: Option<String>,
    pub output: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CatInboundSettings {
    pub network: Option<String>,
    pub address: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShadowsocksInboundSettings {
    pub method: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrojanInboundSettings {
    pub passwords: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WebSocketInboundSettings {
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AMuxInboundSettings {
    pub actors: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicInboundSettings {
    pub certificate: Option<String>,
    #[serde(rename = "certificateKey")]
    pub certificate_key: Option<String>,
    pub alpn: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TlsInboundSettings {
    pub certificate: Option<String>,
    #[serde(rename = "certificateKey")]
    pub certificate_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChainInboundSettings {
    pub actors: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TunInboundSettings {
    pub auto: Option<bool>,
    pub fd: Option<i32>,
    pub name: Option<String>,
    pub address: Option<String>,
    pub gateway: Option<String>,
    pub netmask: Option<String>,
    pub mtu: Option<i32>,
    #[serde(rename = "fakeDnsExclude")]
    pub fake_dns_exclude: Option<Vec<String>>,
    #[serde(rename = "fakeDnsInclude")]
    pub fake_dns_include: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Inbound {
    pub protocol: String,
    pub tag: Option<String>,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub settings: Option<Box<RawValue>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RedirectOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SocksOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShadowsocksOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub method: Option<String>,
    pub password: Option<String>,
    pub prefix: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ObfsOutboundSettings {
    pub method: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrojanOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VMessOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub uuid: Option<String>,
    pub security: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TryAllOutboundSettings {
    pub actors: Option<Vec<String>>,
    #[serde(rename = "delayBase")]
    pub delay_base: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StaticOutboundSettings {
    pub actors: Option<Vec<String>>,
    pub method: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TlsOutboundSettings {
    #[serde(rename = "serverName")]
    pub server_name: Option<String>,
    pub alpn: Option<Vec<String>>,
    pub certificate: Option<String>,
    pub insecure: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WebSocketOutboundSettings {
    pub path: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AMuxOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub actors: Option<Vec<String>>,
    #[serde(rename = "maxAccepts")]
    pub max_accepts: Option<u32>,
    pub concurrency: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuicOutboundSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    #[serde(rename = "serverName")]
    pub server_name: Option<String>,
    pub certificate: Option<String>,
    pub alpn: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChainOutboundSettings {
    pub actors: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RetryOutboundSettings {
    pub actors: Option<Vec<String>>,
    pub attempts: Option<u32>,
}

#[cfg(feature = "outbound-private-tun")]
#[derive(Serialize, Deserialize, Debug)]
pub struct PrivateTunOutboundSettings {
    // 直接使用private_tun的完整ClientConfig
    #[serde(flatten)]
    pub client_config: private_tun::snell_impl_ver::config::ClientConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FailOverOutboundSettings {
    pub actors: Option<Vec<String>>,
    #[serde(rename = "failTimeout")]
    pub fail_timeout: Option<u32>,
    #[serde(rename = "healthCheck")]
    pub health_check: Option<bool>,
    #[serde(rename = "healthCheckTimeout")]
    pub health_check_timeout: Option<u32>,
    #[serde(rename = "healthCheckDelay")]
    pub health_check_delay: Option<u32>,
    #[serde(rename = "healthCheckActive")]
    pub health_check_active: Option<u32>,
    #[serde(rename = "healthCheckPrefers")]
    pub health_check_prefers: Option<Vec<String>>,
    #[serde(rename = "checkInterval")]
    pub check_interval: Option<u32>,
    #[serde(rename = "healthCheckOnStart")]
    pub health_check_on_start: Option<bool>,
    #[serde(rename = "healthCheckWait")]
    pub health_check_wait: Option<bool>,
    #[serde(rename = "healthCheckAttempts")]
    pub health_check_attempts: Option<u32>,
    #[serde(rename = "healthCheckSuccessPercentage")]
    pub health_check_success_percentage: Option<u32>,
    pub failover: Option<bool>,
    #[serde(rename = "fallbackCache")]
    pub fallback_cache: Option<bool>,
    #[serde(rename = "cacheSize")]
    pub cache_size: Option<u32>,
    #[serde(rename = "cacheTimeout")]
    pub cache_timeout: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SelectOutboundSettings {
    pub actors: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginOutboundSettings {
    pub path: Option<String>,
    pub args: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Outbound {
    pub protocol: String,
    pub tag: Option<String>,
    pub settings: Option<Box<RawValue>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Rule {
    pub ip: Option<Vec<String>>,
    pub domain: Option<Vec<String>>,
    #[serde(rename = "domainKeyword")]
    pub domain_keyword: Option<Vec<String>>,
    #[serde(rename = "domainSuffix")]
    pub domain_suffix: Option<Vec<String>>,
    pub geoip: Option<Vec<String>>,
    pub external: Option<Vec<String>>,
    #[serde(rename = "portRange")]
    pub port_range: Option<Vec<String>>,
    pub network: Option<Vec<String>>,
    #[serde(rename = "inboundTag")]
    pub inbound_tag: Option<Vec<String>>,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    pub rules: Option<Vec<Rule>>,
    #[serde(rename = "domainResolve")]
    pub domain_resolve: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub log: Option<Log>,
    pub inbounds: Option<Vec<Inbound>>,
    pub outbounds: Option<Vec<Outbound>>,
    pub router: Option<Router>,
    pub dns: Option<Dns>,
}

pub fn to_internal(json: &mut Config) -> Result<internal::Config> {
    let mut log = internal::Log::new();
    if let Some(ext_log) = &json.log {
        if let Some(ext_level) = &ext_log.level {
            match ext_level.to_lowercase().as_str() {
                "trace" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::TRACE),
                "debug" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::DEBUG),
                "info" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::INFO),
                "warn" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::WARN),
                "error" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::ERROR),
                "none" => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::NONE),
                _ => log.level = protobuf::EnumOrUnknown::new(internal::log::Level::WARN),
            }
        }

        if let Some(ext_output) = &ext_log.output {
            match ext_output.as_str() {
                "console" => {
                    log.output = protobuf::EnumOrUnknown::new(internal::log::Output::CONSOLE)
                }
                _ => {
                    log.output = protobuf::EnumOrUnknown::new(internal::log::Output::FILE);
                    log.output_file = ext_output.clone();
                }
            }
        }
    }

    let mut inbounds = Vec::new();
    if let Some(ext_inbounds) = &json.inbounds {
        for ext_inbound in ext_inbounds {
            let mut inbound = internal::Inbound::new();
            inbound.protocol = ext_inbound.protocol.clone();
            if let Some(ext_tag) = &ext_inbound.tag {
                inbound.tag = ext_tag.clone();
            }
            if let Some(ext_address) = &ext_inbound.address {
                inbound.address = ext_address.to_owned();
            } else {
                inbound.address = "127.0.0.1".to_string();
            }
            if let Some(ext_port) = ext_inbound.port {
                inbound.port = ext_port as u32;
            }
            match inbound.protocol.as_str() {
                "tun" => {
                    if ext_inbound.settings.is_none() {
                        return Err(anyhow!("invalid tun inbound settings"));
                    }
                    let mut settings = internal::TunInboundSettings::new();
                    let ext_settings: TunInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();

                    let mut fake_dns_exclude = Vec::new();
                    if let Some(ext_excludes) = ext_settings.fake_dns_exclude {
                        for ext_exclude in ext_excludes {
                            fake_dns_exclude.push(ext_exclude);
                        }
                    }
                    if !fake_dns_exclude.is_empty() {
                        settings.fake_dns_exclude = fake_dns_exclude;
                    }

                    let mut fake_dns_include = Vec::new();
                    if let Some(ext_includes) = ext_settings.fake_dns_include {
                        for ext_include in ext_includes {
                            fake_dns_include.push(ext_include);
                        }
                    }
                    if !fake_dns_include.is_empty() {
                        settings.fake_dns_include = fake_dns_include;
                    }

                    if let Some(ext_fd) = ext_settings.fd {
                        settings.fd = ext_fd;
                    } else {
                        settings.fd = -1; // disable fd option
                        if let Some(ext_name) = ext_settings.name {
                            settings.name = ext_name;
                        }
                        if let Some(ext_address) = ext_settings.address {
                            settings.address = ext_address;
                        }
                        if let Some(ext_gateway) = ext_settings.gateway {
                            settings.gateway = ext_gateway;
                        }
                        if let Some(ext_netmask) = ext_settings.netmask {
                            settings.netmask = ext_netmask;
                        }
                        if let Some(ext_auto) = ext_settings.auto {
                            settings.auto = ext_auto;
                        }
                        if let Some(ext_mtu) = ext_settings.mtu {
                            settings.mtu = ext_mtu;
                        } else {
                            settings.mtu = 1500;
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "cat" => {
                    let mut settings = internal::CatInboundSettings::new();
                    let ext_settings: CatInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    settings.network = ext_settings.network.unwrap_or("tcp".to_string());
                    settings.address = ext_settings.address;
                    settings.port = ext_settings.port as u32;
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "http" => {
                    inbounds.push(inbound);
                }
                "socks" => {
                    inbounds.push(inbound);
                }
                "shadowsocks" => {
                    let mut settings = internal::ShadowsocksInboundSettings::new();
                    let ext_settings: ShadowsocksInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    if let Some(ext_method) = ext_settings.method {
                        settings.method = ext_method;
                    } else {
                        settings.method = "chacha20-ietf-poly1305".to_string();
                    }
                    if let Some(ext_password) = ext_settings.password {
                        settings.password = ext_password;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "trojan" => {
                    let mut settings = internal::TrojanInboundSettings::new();
                    let ext_settings: TrojanInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    if let Some(ext_passwords) = ext_settings.passwords {
                        for ext_pass in ext_passwords {
                            settings.passwords.push(ext_pass);
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "ws" => {
                    let mut settings = internal::WebSocketInboundSettings::new();
                    let ext_settings: WebSocketInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    match ext_settings.path {
                        Some(ext_path) if !ext_path.is_empty() => {
                            settings.path = ext_path;
                        }
                        _ => {
                            settings.path = "/".to_string();
                        }
                    };
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "amux" => {
                    let mut settings = internal::AMuxInboundSettings::new();
                    if let Some(ext_settings) = &ext_inbound.settings {
                        if let Ok(ext_settings) =
                            serde_json::from_str::<AMuxInboundSettings>(ext_settings.get())
                        {
                            if let Some(ext_actors) = ext_settings.actors {
                                for ext_actor in ext_actors {
                                    settings.actors.push(ext_actor);
                                }
                            }
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "quic" => {
                    let mut settings = internal::QuicInboundSettings::new();
                    let ext_settings: QuicInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    if let Some(ext_certificate) = ext_settings.certificate {
                        let cert = Path::new(&ext_certificate);
                        if cert.is_absolute() {
                            settings.certificate = cert.to_string_lossy().to_string();
                        } else {
                            let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                            let path = asset_loc.join(cert).to_string_lossy().to_string();
                            settings.certificate = path;
                        }
                    }
                    if let Some(ext_certificate_key) = ext_settings.certificate_key {
                        let key = Path::new(&ext_certificate_key);
                        if key.is_absolute() {
                            settings.certificate_key = key.to_string_lossy().to_string();
                        } else {
                            let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                            let path = asset_loc.join(key).to_string_lossy().to_string();
                            settings.certificate_key = path;
                        }
                    }
                    if let Some(ext_alpns) = ext_settings.alpn {
                        settings.alpn = ext_alpns.clone();
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "tls" => {
                    let mut settings = internal::TlsInboundSettings::new();
                    let ext_settings: TlsInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    if let Some(ext_certificate) = ext_settings.certificate {
                        let cert = Path::new(&ext_certificate);
                        if cert.is_absolute() {
                            settings.certificate = cert.to_string_lossy().to_string();
                        } else {
                            let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                            let path = asset_loc.join(cert).to_string_lossy().to_string();
                            settings.certificate = path;
                        }
                    }
                    if let Some(ext_certificate_key) = ext_settings.certificate_key {
                        let key = Path::new(&ext_certificate_key);
                        if key.is_absolute() {
                            settings.certificate_key = key.to_string_lossy().to_string();
                        } else {
                            let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                            let path = asset_loc.join(key).to_string_lossy().to_string();
                            settings.certificate_key = path;
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                "chain" => {
                    if ext_inbound.settings.is_none() {
                        return Err(anyhow!("invalid chain inbound settings"));
                    }
                    let mut settings = internal::ChainInboundSettings::new();
                    let ext_settings: ChainInboundSettings =
                        serde_json::from_str(ext_inbound.settings.as_ref().unwrap().get()).unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    inbound.settings = settings;
                    inbounds.push(inbound);
                }
                _ => {
                    // skip inbound with unknown protocol
                }
            }
        }
    }

    let mut outbounds = Vec::new();
    if let Some(ext_outbounds) = &json.outbounds {
        for ext_outbound in ext_outbounds.iter() {
            let mut outbound = internal::Outbound::new();
            outbound.protocol = ext_outbound.protocol.clone();
            if let Some(ext_tag) = &ext_outbound.tag {
                outbound.tag = ext_tag.to_owned();
            }
            match outbound.protocol.as_str() {
                "direct" | "drop" => {
                    outbounds.push(outbound);
                }
                "redirect" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid redirect outbound settings"));
                    }
                    let mut settings = internal::RedirectOutboundSettings::new();
                    let ext_settings: RedirectOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_address) = ext_settings.address {
                        settings.address = ext_address;
                    }
                    if let Some(ext_port) = ext_settings.port {
                        settings.port = ext_port as u32;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "socks" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid socks outbound settings"));
                    }
                    let mut settings = internal::SocksOutboundSettings::new();
                    let ext_settings: SocksOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_address) = ext_settings.address {
                        settings.address = ext_address; // TODO checks
                    }
                    if let Some(ext_port) = ext_settings.port {
                        settings.port = ext_port as u32; // TODO checks
                    }
                    if let Some(ext_username) = ext_settings.username {
                        settings.username = ext_username;
                    }
                    if let Some(ext_password) = ext_settings.password {
                        settings.password = ext_password;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "shadowsocks" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid shadowsocks outbound settings"));
                    }
                    let mut settings = internal::ShadowsocksOutboundSettings::new();
                    let ext_settings: ShadowsocksOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_address) = ext_settings.address {
                        settings.address = ext_address; // TODO checks
                    }
                    if let Some(ext_port) = ext_settings.port {
                        settings.port = ext_port as u32; // TODO checks
                    }
                    if let Some(ext_method) = ext_settings.method {
                        settings.method = ext_method;
                    } else {
                        settings.method = "chacha20-ietf-poly1305".to_string();
                    }
                    if let Some(ext_password) = ext_settings.password {
                        settings.password = ext_password;
                    }
                    settings.prefix = ext_settings.prefix.as_ref().cloned();
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "obfs" => {
                    let mut settings = internal::ObfsOutboundSettings::new();
                    let ext_settings: ObfsOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_method) = ext_settings.method {
                        // TODO checks
                        settings.method = ext_method;
                    } else {
                        settings.method = "http".to_string();
                    }
                    if let Some(ext_host) = ext_settings.host {
                        settings.host = ext_host;
                    }
                    if let Some(ext_path) = ext_settings.path {
                        settings.path = ext_path;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "trojan" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid trojan outbound settings"));
                    }
                    let mut settings = internal::TrojanOutboundSettings::new();
                    let ext_settings: TrojanOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_address) = ext_settings.address {
                        settings.address = ext_address; // TODO checks
                    }
                    if let Some(ext_port) = ext_settings.port {
                        settings.port = ext_port as u32; // TODO checks
                    }
                    if let Some(ext_password) = ext_settings.password {
                        settings.password = ext_password;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "vmess" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid vmess outbound settings"));
                    }
                    let mut settings = internal::VMessOutboundSettings::new();
                    let ext_settings: VMessOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    settings.address = ext_settings.address.unwrap_or_default();
                    settings.port = ext_settings.port.map(|x| x as u32).unwrap_or_default();
                    settings.uuid = ext_settings.uuid.unwrap_or_default();
                    settings.security = ext_settings
                        .security
                        .unwrap_or(String::from("chacha20-ietf-poly1305"));
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "tls" => {
                    let mut settings = internal::TlsOutboundSettings::new();
                    if ext_outbound.settings.is_some() {
                        let ext_settings: TlsOutboundSettings =
                            serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                                .unwrap();
                        if let Some(ext_server_name) = ext_settings.server_name {
                            settings.server_name = ext_server_name; // TODO checks
                        }
                        let mut alpns = Vec::new();
                        if let Some(ext_alpns) = ext_settings.alpn {
                            for ext_alpn in ext_alpns {
                                alpns.push(ext_alpn);
                            }
                        }
                        if !alpns.is_empty() {
                            settings.alpn = alpns;
                        }
                        if let Some(ext_certificate) = ext_settings.certificate {
                            let cert = Path::new(&ext_certificate);
                            if cert.is_absolute() {
                                settings.certificate = cert.to_string_lossy().to_string();
                            } else {
                                let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                                let path = asset_loc.join(cert).to_string_lossy().to_string();
                                settings.certificate = path;
                            }
                        }
                        settings.insecure = ext_settings.insecure.unwrap_or_default();
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "ws" | "websocket" => {
                    outbound.protocol = "ws".to_string(); // websocket -> ws
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid ws outbound settings"));
                    }
                    let mut settings = internal::WebSocketOutboundSettings::new();
                    let ext_settings: WebSocketOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_path) = ext_settings.path {
                        settings.path = ext_path; // TODO checks
                    }
                    if let Some(ext_headers) = ext_settings.headers {
                        settings.headers = ext_headers;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "tryall" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid tryall outbound settings"));
                    }
                    let mut settings = internal::TryAllOutboundSettings::new();
                    let ext_settings: TryAllOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    if let Some(ext_delay_base) = ext_settings.delay_base {
                        settings.delay_base = ext_delay_base;
                    } else {
                        settings.delay_base = 0;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "static" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid static outbound settings"));
                    }
                    let mut settings = internal::StaticOutboundSettings::new();
                    let ext_settings: StaticOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    if let Some(ext_method) = &ext_settings.method {
                        settings.method = ext_method.clone();
                    } else {
                        settings.method = "random".to_string();
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "failover" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid failover outbound settings"));
                    }
                    let mut settings = internal::FailOverOutboundSettings::new();
                    let ext_settings: FailOverOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        settings.actors.extend_from_slice(&ext_actors);
                    }
                    settings.fail_timeout = ext_settings.fail_timeout.unwrap_or(4); // 4 secs
                    settings.health_check = ext_settings.health_check.unwrap_or(true);
                    settings.health_check_timeout = ext_settings.health_check_timeout.unwrap_or(6); // 6 secs
                    settings.health_check_delay = ext_settings.health_check_delay.unwrap_or(200); // 200ms
                    settings.health_check_active =
                        ext_settings.health_check_active.unwrap_or(15 * 60); // 15 mins
                    if let Some(ext_health_check_prefers) = ext_settings.health_check_prefers {
                        settings
                            .health_check_prefers
                            .extend_from_slice(&ext_health_check_prefers);
                    }
                    settings.health_check_on_start =
                        ext_settings.health_check_on_start.unwrap_or(false);
                    settings.health_check_wait = ext_settings.health_check_wait.unwrap_or(false);
                    settings.health_check_attempts =
                        ext_settings.health_check_attempts.unwrap_or(1);
                    settings.health_check_success_percentage =
                        ext_settings.health_check_success_percentage.unwrap_or(50);
                    settings.check_interval = ext_settings.check_interval.unwrap_or(300); // 300 secs
                    settings.failover = ext_settings.failover.unwrap_or(true);
                    settings.fallback_cache = ext_settings.fallback_cache.unwrap_or(false);
                    settings.cache_size = ext_settings.cache_size.unwrap_or(256);
                    settings.cache_timeout = ext_settings.cache_timeout.unwrap_or(60); // 60 mins
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "amux" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid amux outbound settings"));
                    }
                    let mut settings = internal::AMuxOutboundSettings::new();
                    let ext_settings: AMuxOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_address) = ext_settings.address {
                        settings.address = ext_address;
                    }
                    if let Some(ext_port) = ext_settings.port {
                        settings.port = ext_port as u32;
                    }
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    if let Some(ext_max_accepts) = ext_settings.max_accepts {
                        settings.max_accepts = ext_max_accepts;
                    } else {
                        settings.max_accepts = 8;
                    }
                    if let Some(ext_concurrency) = ext_settings.concurrency {
                        settings.concurrency = ext_concurrency;
                    } else {
                        settings.concurrency = 2;
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "quic" => {
                    let mut settings = internal::QuicOutboundSettings::new();
                    if ext_outbound.settings.is_some() {
                        let ext_settings: QuicOutboundSettings =
                            serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                                .unwrap();
                        if let Some(ext_address) = ext_settings.address {
                            settings.address = ext_address;
                        }
                        if let Some(ext_port) = ext_settings.port {
                            settings.port = ext_port as u32;
                        }
                        if let Some(ext_server_name) = ext_settings.server_name {
                            settings.server_name = ext_server_name;
                        }
                        if let Some(ext_certificate) = ext_settings.certificate {
                            let cert = Path::new(&ext_certificate);
                            if cert.is_absolute() {
                                settings.certificate = cert.to_string_lossy().to_string();
                            } else {
                                let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                                let path = asset_loc.join(cert).to_string_lossy().to_string();
                                settings.certificate = path;
                            }
                        }
                        if let Some(ext_alpns) = ext_settings.alpn {
                            settings.alpn = ext_alpns.clone();
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "chain" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid chain outbound settings"));
                    }
                    let mut settings = internal::ChainOutboundSettings::new();
                    let ext_settings: ChainOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "select" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid select outbound settings"));
                    }
                    let mut settings = internal::SelectOutboundSettings::new();
                    let ext_settings: SelectOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_actors) = ext_settings.actors {
                        for ext_actor in ext_actors {
                            settings.actors.push(ext_actor);
                        }
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
#[cfg(feature = "outbound-private-tun")]
                "private-tun" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid private-tun outbound settings"));
                    }
                    let mut settings = internal::PrivateTunOutboundSettings::new();
                    let ext_settings: PrivateTunOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    
                    // 将完整的ClientConfig序列化为JSON存储
                    let client_config_json = serde_json::to_string(&ext_settings.client_config)?;
                    settings.client_config_json = client_config_json;
                    
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                "plugin" => {
                    if ext_outbound.settings.is_none() {
                        return Err(anyhow!("invalid plugin outbound settings"));
                    }
                    let mut settings = internal::PluginOutboundSettings::new();
                    let ext_settings: PluginOutboundSettings =
                        serde_json::from_str(ext_outbound.settings.as_ref().unwrap().get())
                            .unwrap();
                    if let Some(ext_path) = ext_settings.path {
                        settings.path = ext_path; // TODO checks
                    }
                    if let Some(ext_args) = ext_settings.args {
                        settings.args = ext_args; // TODO checks
                    }
                    let settings = settings.write_to_bytes().unwrap();
                    outbound.settings = settings;
                    outbounds.push(outbound);
                }
                _ => {
                    // skip outbound with unknown protocol
                }
            }
        }
    }

    let mut router = protobuf::MessageField::none();
    if let Some(ext_router) = json.router.as_mut() {
        let mut int_router = internal::Router::new();
        let mut rules = Vec::new();
        if let Some(ext_rules) = ext_router.rules.as_mut() {
            // a map for caching external site so we need not load a same file multiple times
            for ext_rule in ext_rules.iter_mut() {
                let mut rule = internal::router::Rule::new();
                let target_tag = std::mem::take(&mut ext_rule.target);
                rule.target_tag = target_tag;
                if let Some(ext_ips) = ext_rule.ip.as_mut() {
                    for ext_ip in ext_ips.drain(0..) {
                        rule.ip_cidrs.push(ext_ip);
                    }
                }
                if let Some(ext_domains) = ext_rule.domain.as_mut() {
                    for ext_domain in ext_domains.drain(0..) {
                        let mut domain = internal::router::rule::Domain::new();
                        domain.type_ = protobuf::EnumOrUnknown::new(
                            internal::router::rule::domain::Type::FULL,
                        );
                        domain.value = ext_domain;
                        rule.domains.push(domain);
                    }
                }
                if let Some(ext_domain_keywords) = ext_rule.domain_keyword.as_mut() {
                    for ext_domain_keyword in ext_domain_keywords.drain(0..) {
                        let mut domain = internal::router::rule::Domain::new();
                        domain.type_ = protobuf::EnumOrUnknown::new(
                            internal::router::rule::domain::Type::PLAIN,
                        );
                        domain.value = ext_domain_keyword;
                        rule.domains.push(domain);
                    }
                }
                if let Some(ext_domain_suffixes) = ext_rule.domain_suffix.as_mut() {
                    for ext_domain_suffix in ext_domain_suffixes.drain(0..) {
                        let mut domain = internal::router::rule::Domain::new();
                        domain.type_ = protobuf::EnumOrUnknown::new(
                            internal::router::rule::domain::Type::DOMAIN,
                        );
                        domain.value = ext_domain_suffix;
                        rule.domains.push(domain);
                    }
                }
                if let Some(ext_geoips) = ext_rule.geoip.as_mut() {
                    for ext_geoip in ext_geoips.drain(0..) {
                        let mut mmdb = internal::router::rule::Mmdb::new();
                        let asset_loc = Path::new(&*crate::option::ASSET_LOCATION);
                        mmdb.file = asset_loc.join("geo.mmdb").to_string_lossy().to_string();
                        mmdb.country_code = ext_geoip;
                        rule.mmdbs.push(mmdb)
                    }
                }
                if let Some(ext_externals) = ext_rule.external.as_mut() {
                    for ext_external in ext_externals.drain(0..) {
                        match external_rule::add_external_rule(&mut rule, &ext_external) {
                            Ok(_) => (),
                            Err(e) => {
                                println!("load external rule failed: {}", e);
                            }
                        }
                    }
                }
                if let Some(ext_port_ranges) = ext_rule.port_range.as_mut() {
                    for ext_port_range in ext_port_ranges.drain(0..) {
                        // FIXME validate
                        rule.port_ranges.push(ext_port_range);
                    }
                }
                if let Some(ext_networks) = ext_rule.network.as_mut() {
                    for ext_network in ext_networks.drain(0..) {
                        // FIXME validate
                        rule.networks.push(ext_network);
                    }
                }
                if let Some(ext_its) = ext_rule.inbound_tag.as_mut() {
                    for it in ext_its.drain(0..) {
                        rule.inbound_tags.push(it);
                    }
                }
                rules.push(rule);
            }
        }
        int_router.rules = rules;
        if let Some(ext_domain_resolve) = ext_router.domain_resolve {
            int_router.domain_resolve = ext_domain_resolve;
        }
        router = protobuf::MessageField::some(int_router);
    }

    let mut dns = internal::Dns::new();
    let mut servers = Vec::new();
    let mut hosts = HashMap::new();
    if let Some(ext_dns) = &json.dns {
        if let Some(ext_servers) = ext_dns.servers.as_ref() {
            for ext_server in ext_servers {
                servers.push(ext_server.to_owned());
            }
        }
        if let Some(ext_hosts) = ext_dns.hosts.as_ref() {
            for (name, static_ips) in ext_hosts.iter() {
                let mut ips = internal::dns::Ips::new();
                let mut ip_vals = Vec::new();
                for ip in static_ips {
                    ip_vals.push(ip.to_owned());
                }
                ips.values = ip_vals;
                hosts.insert(name.to_owned(), ips);
            }
        }
    }
    if servers.is_empty() {
        servers.push("1.1.1.1".to_string());
    }
    dns.servers = servers;
    if !hosts.is_empty() {
        dns.hosts = hosts;
    }

    let mut config = internal::Config::new();
    config.log = protobuf::MessageField::some(log);
    config.inbounds = inbounds;
    config.outbounds = outbounds;
    config.router = router;
    config.dns = protobuf::MessageField::some(dns);
    Ok(config)
}

pub fn json_from_string(config: &str) -> Result<Config> {
    serde_json::from_str(config).map_err(|e| anyhow!("deserialize json config failed: {}", e))
}

pub fn from_string(s: &str) -> Result<internal::Config> {
    let mut config = json_from_string(s)?;
    to_internal(&mut config)
}

pub fn from_file<P>(path: P) -> Result<internal::Config>
where
    P: AsRef<Path>,
{
    let config = std::fs::read_to_string(path)?;
    let mut config = json_from_string(&config)?;
    to_internal(&mut config)
}
