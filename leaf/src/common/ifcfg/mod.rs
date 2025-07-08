#[cfg(any(target_os = "macos", target_os = "freebsd"))]
mod darwin;
#[cfg(any(target_os = "linux"))]
mod netlink;
#[cfg(target_os = "windows")]
mod windows;

mod route;

use std::net::{Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use socket2::SockRef;
use tokio::process::Command;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("shell command error: {0}")]
    ShellCommandError(String),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Not found")]
    NotFound,
    #[error("Timeout error: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

#[async_trait]
pub trait IfConfiguerTrait: Send + Sync {
    async fn add_ipv4_route(
        &self,
        _name: &str,
        _address: Ipv4Addr,
        _cidr_prefix: u8,
        _cost: Option<i32>,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn remove_ipv4_route(
        &self,
        _name: &str,
        _address: Ipv4Addr,
        _cidr_prefix: u8,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn add_ipv4_ip(
        &self,
        _name: &str,
        _address: Ipv4Addr,
        _cidr_prefix: u8,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn set_link_status(&self, _name: &str, _up: bool) -> Result<(), Error> {
        Ok(())
    }
    async fn remove_ip(&self, _name: &str, _ip: Option<Ipv4Addr>) -> Result<(), Error> {
        Ok(())
    }
    async fn wait_interface_show(&self, _name: &str) -> Result<(), Error> {
        return Ok(());
    }
    async fn set_mtu(&self, _name: &str, _mtu: u32) -> Result<(), Error> {
        Ok(())
    }
}

fn cidr_to_subnet_mask(prefix_length: u8) -> Ipv4Addr {
    if prefix_length > 32 {
        panic!("Invalid CIDR prefix length");
    }

    let subnet_mask: u32 = (!0u32)
        .checked_shl(32 - u32::from(prefix_length))
        .unwrap_or(0);
    Ipv4Addr::new(
        ((subnet_mask >> 24) & 0xFF) as u8,
        ((subnet_mask >> 16) & 0xFF) as u8,
        ((subnet_mask >> 8) & 0xFF) as u8,
        (subnet_mask & 0xFF) as u8,
    )
}

async fn run_shell_cmd(cmd: &str) -> Result<(), Error> {
    let cmd_out: std::process::Output;
    let stdout: String;
    let stderr: String;
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd_out = Command::new("cmd")
            .stdin(std::process::Stdio::null())
            .arg("/C")
            .arg(cmd)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .await?;
        stdout = crate::common::arch::windows::utf8_or_gbk_to_string(cmd_out.stdout.as_slice());
        stderr = crate::common::arch::windows::utf8_or_gbk_to_string(cmd_out.stderr.as_slice());
    };

    #[cfg(not(target_os = "windows"))]
    {
        cmd_out = Command::new("sh").arg("-c").arg(cmd).output().await?;
        stdout = String::from_utf8_lossy(cmd_out.stdout.as_slice()).to_string();
        stderr = String::from_utf8_lossy(cmd_out.stderr.as_slice()).to_string();
    };

    let ec = cmd_out.status.code();
    let succ = cmd_out.status.success();
    tracing::info!(?cmd, ?ec, ?succ, ?stdout, ?stderr, "run shell cmd");

    if !cmd_out.status.success() {
        return Err(Error::ShellCommandError(stdout + &stderr));
    }
    Ok(())
}

pub struct DummyIfConfiger {}
#[async_trait]
impl IfConfiguerTrait for DummyIfConfiger {}

#[cfg(any(target_os = "linux"))]
pub type IfConfiger = netlink::NetlinkIfConfiger;

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
pub type IfConfiger = darwin::MacIfConfiger;

#[cfg(target_os = "windows")]
pub type IfConfiger = windows::WindowsIfConfiger;

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows",
    target_os = "freebsd",
)))]
pub type IfConfiger = DummyIfConfiger;

#[cfg(target_os = "windows")]
pub use windows::RegistryManager;




pub fn setup_sokcet2_ext(
    socket2_socket: SockRef,
    bind_addr: &SocketAddr,
    #[allow(unused_variables)] bind_dev: Option<String>,
) -> Result<(), Error> {
    #[cfg(target_os = "windows")]
    {
        let is_udp = matches!(socket2_socket.r#type()?, socket2::Type::DGRAM);
        crate::common::arch::windows::setup_socket_for_win(socket2_socket, bind_addr, bind_dev, is_udp)?;
    }

    if let Err(e) = socket2_socket.bind(&socket2::SockAddr::from(*bind_addr)) {
        if bind_addr.is_ipv4() {
            return Err(e.into());
        } else {
            tracing::warn!(?e, "bind failed, do not return error for ipv6");
        }
    }

    // #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    // socket2_socket.set_reuse_port(true)?;

    if bind_addr.ip().is_unspecified() {
        return Ok(());
    }

    // linux/mac does not use interface of bind_addr to send packet, so we need to bind device
    // win can handle this with bind correctly
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    if let Some(dev_name) = bind_dev {
        // use IP_BOUND_IF to bind device
        unsafe {
            let dev_idx = nix::libc::if_nametoindex(dev_name.as_str().as_ptr() as *const i8);
            tracing::warn!(?dev_idx, ?dev_name, "bind device");
            socket2_socket.bind_device_by_index_v4(std::num::NonZeroU32::new(dev_idx))?;
            tracing::warn!(?dev_idx, ?dev_name, "bind device doen");
        }
    }

    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    if let Some(dev_name) = bind_dev {
        tracing::trace!(dev_name = ?dev_name, "bind device");
        if let Err(e) = socket2_socket.bind_device(Some(dev_name.as_bytes())) {
            tracing::warn!(?e, dev_name = ?dev_name, bind_addr = ?bind_addr, "bind device failed");
        }
    }

    Ok(())
}