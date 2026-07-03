use anyhow::{Context, Result};
use zbus::Connection;

use crate::{config::SshConfig, util};

pub fn targets_ok(cfg: &SshConfig) -> bool {
    util::multi_target_probe(&cfg.targets, cfg.require_all, cfg.ssh_timeout)
}

// systemd D-Bus: ActiveState == "active"
pub async fn systemd_unit_is_active(unit: &str) -> Result<bool> {
    let conn = Connection::system()
        .await
        .context("connecting to system D-Bus")?;

    let manager = zbus::Proxy::new(
        &conn,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .context("creating systemd manager proxy")?;

    let unit_path: zbus::zvariant::OwnedObjectPath =
        manager.call("GetUnit", &(unit)).await.context("GetUnit")?;

    let unit_proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .context("creating systemd unit proxy")?;

    let active_state: String = unit_proxy
        .get_property("ActiveState")
        .await
        .context("reading ActiveState")?;

    Ok(active_state == "active")
}
