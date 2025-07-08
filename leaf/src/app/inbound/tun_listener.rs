use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::app::dispatcher::Dispatcher;
use crate::app::nat_manager::NatManager;
use crate::config::Inbound;
use crate::proxy::tun::{self as self_tun, TunInfo};
use crate::Runner;

pub struct TunInboundListener {
    pub inbound: Inbound,
    pub dispatcher: Arc<Dispatcher>,
    pub nat_manager: Arc<NatManager>,
}

impl TunInboundListener {
    pub fn listen(&self) -> Result<(Runner, TunInfo)> {
        let (runner, tun_cfg) = self_tun::inbound::new(
            self.inbound.clone(),
            self.dispatcher.clone(),
            self.nat_manager.clone(),
        )?;
        Ok((runner, tun_cfg))
    }
}
