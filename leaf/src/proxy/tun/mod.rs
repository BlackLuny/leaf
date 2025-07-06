

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


