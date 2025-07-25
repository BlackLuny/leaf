use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command;

use anyhow::Result;

pub fn get_default_ipv4_gateway() -> Result<String> {
    let out = Command::new("route")
        .arg("-n")
        .arg("get")
        .arg("1")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    assert!(out.status.success());
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    let cols: Vec<&str> = out
        .lines()
        .find(|l| l.contains("gateway"))
        .ok_or_else(|| anyhow::anyhow!("no gateway found"))?
        .split_whitespace()
        .map(str::trim)
        .collect();
    assert!(cols.len() == 2);
    let res = cols[1].to_string();
    Ok(res)
}

pub fn get_default_ipv6_gateway() -> Result<String> {
    let out = Command::new("route")
        .arg("-n")
        .arg("get")
        .arg("-inet6")
        .arg("::2")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    assert!(out.status.success());
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    let cols: Vec<&str> = out
        .lines()
        .find(|l| l.contains("gateway"))
        .ok_or_else(|| anyhow::anyhow!("no gateway found"))?
        .split_whitespace()
        .map(str::trim)
        .collect();
    assert!(cols.len() == 2);
    let parts: Vec<&str> = cols[1].split('%').map(str::trim).collect();
    assert!(!parts.is_empty());
    let res = parts[0].to_string();
    Ok(res)
}

pub fn get_default_interface() -> Result<String> {
    let out = Command::new("route")
        .arg("-n")
        .arg("get")
        .arg("1")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    assert!(out.status.success());
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    let cols: Vec<&str> = out
        .lines()
        .find(|l| l.contains("interface"))
        .ok_or_else(|| anyhow::anyhow!("no interface found"))?
        .split_whitespace()
        .map(str::trim)
        .collect();
    assert!(cols.len() == 2);
    let res = cols[1].to_string();
    Ok(res)
}

pub fn add_interface_ipv4_address(
    name: &str,
    addr: Ipv4Addr,
    gw: Ipv4Addr,
    mask: Ipv4Addr,
) -> Result<()> {
    Command::new("ifconfig")
        .arg(name)
        .arg("inet")
        .arg(addr.to_string())
        .arg("netmask")
        .arg(mask.to_string())
        .arg(gw.to_string())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    Ok(())
}

pub fn add_interface_ipv6_address(name: &str, addr: Ipv6Addr, prefixlen: i32) -> Result<()> {
    Command::new("ifconfig")
        .arg(name)
        .arg("inet6")
        .arg(addr.to_string())
        .arg("prefixlen")
        .arg(prefixlen.to_string())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    Ok(())
}

pub fn add_default_ipv4_route(gateway: Ipv4Addr, interface: String, primary: bool) -> Result<()> {
    if primary {
        Command::new("route")
            .arg("add")
            .arg("-inet")
            .arg("default")
            .arg(gateway.to_string())
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    } else {
        Command::new("route")
            .arg("add")
            .arg("-inet")
            .arg("default")
            .arg(gateway.to_string())
            .arg("-ifscope")
            .arg(interface)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    };
    Ok(())
}

pub fn add_default_ipv6_route(gateway: Ipv6Addr, interface: String, primary: bool) -> Result<()> {
    // FIXME https://doc.rust-lang.org/std/net/struct.Ipv6Addr.html#method.is_global
    let gw = if (gateway.segments()[0] & 0xffc0) == 0xfe80 {
        format!("{}%{}", gateway, interface)
    } else {
        gateway.to_string()
    };
    if primary {
        Command::new("route")
            .arg("add")
            .arg("-inet6")
            .arg("default")
            .arg(gw)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    } else {
        Command::new("route")
            .arg("add")
            .arg("-inet6")
            .arg("default")
            .arg(gw)
            .arg("-ifscope")
            .arg(interface)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    };
    Ok(())
}

pub fn delete_default_ipv4_route(ifscope: Option<String>) -> Result<()> {
    if let Some(ifscope) = ifscope {
        Command::new("route")
            .arg("delete")
            .arg("-inet")
            .arg("default")
            .arg("-ifscope")
            .arg(ifscope)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    } else {
        Command::new("route")
            .arg("delete")
            .arg("-inet")
            .arg("default")
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    };
    Ok(())
}

pub fn delete_default_ipv6_route(ifscope: Option<String>) -> Result<()> {
    if let Some(ifscope) = ifscope {
        Command::new("route")
            .arg("delete")
            .arg("-inet6")
            .arg("default")
            .arg("-ifscope")
            .arg(ifscope)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    } else {
        Command::new("route")
            .arg("delete")
            .arg("-inet6")
            .arg("default")
            .status()
            .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    };
    Ok(())
}

pub fn get_ipv4_forwarding() -> Result<bool> {
    let out = Command::new("sysctl")
        .arg("-n")
        .arg("net.inet.ip.forwarding")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(out
        .trim()
        .parse::<i8>()
        .map_err(|e| anyhow::anyhow!("unexpected ip_forward value: {}", e))?
        != 0)
}

pub fn get_ipv6_forwarding() -> Result<bool> {
    let out = Command::new("sysctl")
        .arg("-n")
        .arg("net.inet6.ip6.forwarding")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(out
        .trim()
        .parse::<i8>()
        .map_err(|e| anyhow::anyhow!("unexpected ip_forward value: {}", e))?
        != 0)
}

pub fn set_ipv4_forwarding(val: bool) -> Result<()> {
    Command::new("sysctl")
        .arg("-w")
        .arg(format!(
            "net.inet.ip.forwarding={}",
            if val { "1" } else { "0" }
        ))
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    Ok(())
}

pub fn set_ipv6_forwarding(val: bool) -> Result<()> {
    Command::new("sysctl")
        .arg("-w")
        .arg(format!(
            "net.inet6.ip6.forwarding={}",
            if val { "1" } else { "0" }
        ))
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute command: {}", e))?;
    Ok(())
}
