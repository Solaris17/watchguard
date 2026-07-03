use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use zbus::Connection;

use crate::{
    config::{AppConfig, EscalationStep, SshConfig},
    plugin::{CheckState, Plugin, PluginStatus, TickOutcome},
    util,
};

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

pub struct SshServicePlugin {
    cfg: SshConfig,
    state: CheckState,
}

impl SshServicePlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        Self {
            cfg: cfg.ssh.clone(),
            state: CheckState::default(),
        }
    }
}

impl Plugin for SshServicePlugin {
    fn id(&self) -> &'static str {
        "ssh-service"
    }

    fn name(&self) -> &'static str {
        "SSH service"
    }

    fn description(&self) -> &'static str {
        "Monitors the systemd SSH service active state"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled && self.cfg.service_check_enabled
    }

    fn interval(&self) -> Duration {
        self.cfg.service_check_interval
    }

    fn escalation_steps(&self) -> Vec<EscalationStep> {
        self.cfg.service_failure_actions.clone()
    }

    fn failure_reason(&self) -> &'static str {
        "SSH service failure limit exceeded"
    }

    fn success_message(&self) -> &'static str {
        "SSH service recovered"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        self.cfg = cfg.ssh.clone();
    }

    fn probe(&mut self, rt: &Runtime) -> Result<bool> {
        rt.block_on(systemd_unit_is_active(&self.cfg.service))
    }

    fn status(&mut self, rt: &Runtime) -> PluginStatus {
        if !self.enabled() {
            return PluginStatus::disabled(self.id(), "disabled");
        }

        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(self.id(), format!("{} is active", self.cfg.service)),
            Ok(false) => {
                PluginStatus::warning(self.id(), format!("{} is not active", self.cfg.service))
            }
            Err(e) => PluginStatus::warning(self.id(), format!("status error: {:#}", e)),
        }
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(self.id(), format!("{} active", self.cfg.service)),
            Ok(false) => {
                PluginStatus::failed(self.id(), format!("{} not active", self.cfg.service))
            }
            Err(e) => PluginStatus::failed(self.id(), format!("error: {:#}", e)),
        }
    }

    fn tick(&mut self, rt: &Runtime, now: Instant) -> TickOutcome {
        if !self.enabled() {
            self.state.reset_disabled(now);
            return TickOutcome::Idle;
        }

        if !self.state.due(now, self.interval()) {
            return TickOutcome::Idle;
        }

        let result = self.probe(rt);
        let escalation_steps = self.escalation_steps();

        self.state.record(
            self.id(),
            &escalation_steps,
            self.failure_reason(),
            self.success_message(),
            result,
        )
    }
}

pub struct SshTargetsPlugin {
    cfg: SshConfig,
    state: CheckState,
}

impl SshTargetsPlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        Self {
            cfg: cfg.ssh.clone(),
            state: CheckState::default(),
        }
    }
}

impl Plugin for SshTargetsPlugin {
    fn id(&self) -> &'static str {
        "ssh-targets"
    }

    fn name(&self) -> &'static str {
        "SSH targets"
    }

    fn description(&self) -> &'static str {
        "Monitors SSH TCP target reachability"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled && self.cfg.target_check_enabled
    }

    fn interval(&self) -> Duration {
        self.cfg.ssh_check_interval
    }

    fn escalation_steps(&self) -> Vec<EscalationStep> {
        self.cfg.ssh_failure_actions.clone()
    }

    fn failure_reason(&self) -> &'static str {
        "SSH target reachability failure limit exceeded"
    }

    fn success_message(&self) -> &'static str {
        "SSH target reachability recovered"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        self.cfg = cfg.ssh.clone();
    }

    fn probe(&mut self, _rt: &Runtime) -> Result<bool> {
        Ok(targets_ok(&self.cfg))
    }

    fn status(&mut self, rt: &Runtime) -> PluginStatus {
        if !self.enabled() {
            return PluginStatus::disabled(self.id(), "disabled");
        }

        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!("{} target(s) configured", self.cfg.targets.len()),
            ),
            Ok(false) => PluginStatus::warning(
                self.id(),
                format!(
                    "target probe failed, {} target(s) configured",
                    self.cfg.targets.len()
                ),
            ),
            Err(e) => PluginStatus::warning(self.id(), format!("status error: {:#}", e)),
        }
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(self.id(), "target probe succeeded"),
            Ok(false) => PluginStatus::failed(
                self.id(),
                format!("target probe failed: {:?}", self.cfg.targets),
            ),
            Err(e) => PluginStatus::failed(self.id(), format!("error: {:#}", e)),
        }
    }

    fn tick(&mut self, rt: &Runtime, now: Instant) -> TickOutcome {
        if !self.enabled() {
            self.state.reset_disabled(now);
            return TickOutcome::Idle;
        }

        if !self.state.due(now, self.interval()) {
            return TickOutcome::Idle;
        }

        let result = self.probe(rt);
        let escalation_steps = self.escalation_steps();

        self.state.record(
            self.id(),
            &escalation_steps,
            self.failure_reason(),
            self.success_message(),
            result,
        )
    }
}
