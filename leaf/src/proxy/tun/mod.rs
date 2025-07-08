use std::net::IpAddr;

#[cfg(all(feature = "netstack-lwip", not(feature = "netstack-smoltcp")))]
pub use netstack_lwip as netstack;

#[cfg(all(feature = "netstack-smoltcp", not(feature = "netstack-lwip")))]
pub use netstack_smoltcp as netstack;

#[cfg(all(feature = "netstack-smoltcp", not(feature = "netstack-lwip")))]
pub mod inbound_smoltcp;

#[cfg(all(feature = "netstack-lwip", not(feature = "netstack-smoltcp")))]
pub mod inbound_lwip;

#[cfg(all(feature = "netstack-lwip", not(feature = "netstack-smoltcp")))]
pub use self::inbound_lwip as inbound;

#[cfg(all(feature = "netstack-smoltcp", not(feature = "netstack-lwip")))]
pub use self::inbound_smoltcp as inbound;

#[derive(Debug)]
pub struct TunInfo {
    pub tun_name: String,
    pub tun_ip: IpAddr,
    pub tun_netmask: IpAddr,
    pub tun_gateway: IpAddr,
    pub tun_mtu: u16,
}

impl Default for TunInfo {
    fn default() -> Self {
        Self {
            tun_name: "".to_string(),
            tun_ip: "0.0.0.0".parse().unwrap(),
            tun_netmask: "255.255.255.0".parse().unwrap(),
            tun_gateway: "0.0.0.0".parse().unwrap(),
            tun_mtu: 0,
        }
    }
}
