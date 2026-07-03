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

pub fn systemd_unit_load_state(unit: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/systemctl")
        .args(["show", "-p", "LoadState", "--value", unit])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

pub fn systemd_unit_exists(unit: &str) -> bool {
    matches!(
        systemd_unit_load_state(unit).as_deref(),
        Some("loaded") | Some("masked") | Some("static") | Some("generated") | Some("transient")
    )
}

pub fn resolve_ssh_service(configured: &str) -> String {
    let configured = configured.trim();

    if !configured.is_empty() && configured != "auto" {
        return configured.to_string();
    }

    if systemd_unit_exists("sshd.service") {
        return "sshd.service".to_string();
    }

    if systemd_unit_exists("ssh.service") {
        return "ssh.service".to_string();
    }

    // Fall back to the RHEL-style name so the eventual systemd error is explicit.
    "sshd.service".to_string()
}

pub fn is_auto(value: &str) -> bool {
    value.trim() == "auto"
}
