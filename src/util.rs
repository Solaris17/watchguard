use anyhow::{anyhow, Context, Result};
use std::{
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::Path,
    time::Duration,
};

pub fn command_exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn validate_socket_addr(label: &str, target: &str) -> Result<()> {
    target
        .parse::<SocketAddr>()
        .with_context(|| format!("{} {:?} must be in ip:port format", label, target))?;
    Ok(())
}

pub fn validate_host_port(label: &str, target: &str) -> Result<()> {
    if target.parse::<SocketAddr>().is_ok() {
        return Ok(());
    }

    if let Some((host, port)) = target.rsplit_once(':') {
        if !host.trim().is_empty() && port.parse::<u16>().is_ok() {
            return Ok(());
        }
    }

    Err(anyhow!(
        "{} target {:?} must be in host:port format",
        label,
        target
    ))
}

pub fn validate_targets(label: &str, targets: &[String]) -> Result<()> {
    for target in targets {
        validate_host_port(label, target)?;
    }
    Ok(())
}

pub fn tcp_probe(addr: &str, timeout: Duration) -> bool {
    let addrs = match addr.to_socket_addrs() {
        Ok(v) => v.collect::<Vec<_>>(),
        Err(_) => return false,
    };

    if addrs.is_empty() {
        return false;
    }

    addrs
        .into_iter()
        .any(|socket_addr| TcpStream::connect_timeout(&socket_addr, timeout).is_ok())
}

pub fn multi_target_probe(targets: &[String], require_all: bool, timeout: Duration) -> bool {
    if require_all {
        targets.iter().all(|t| tcp_probe(t, timeout))
    } else {
        targets.iter().any(|t| tcp_probe(t, timeout))
    }
}
