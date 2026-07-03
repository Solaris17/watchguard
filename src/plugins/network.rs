use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::{
    config::{AppConfig, EscalationStep, NetworkConfig},
    plugin::{CheckState, Plugin, PluginStatus, TickOutcome},
    util,
};

pub fn check(cfg: &NetworkConfig) -> bool {
    util::multi_target_probe(&cfg.targets, cfg.require_all, cfg.timeout)
}

pub struct NetworkPlugin {
    cfg: NetworkConfig,
    state: CheckState,
}

impl NetworkPlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        Self {
            cfg: cfg.network.clone(),
            state: CheckState::default(),
        }
    }
}

impl Plugin for NetworkPlugin {
    fn id(&self) -> &'static str {
        "network"
    }

    fn name(&self) -> &'static str {
        "Network"
    }

    fn description(&self) -> &'static str {
        "Monitors outbound TCP reachability"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn interval(&self) -> Duration {
        self.cfg.check_interval
    }

    fn escalation_steps(&self) -> Vec<EscalationStep> {
        self.cfg.failure_actions.clone()
    }

    fn failure_reason(&self) -> &'static str {
        "Network failure limit exceeded"
    }

    fn success_message(&self) -> &'static str {
        "Network reachability recovered"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        self.cfg = cfg.network.clone();
    }

    fn probe(&mut self, _rt: &Runtime) -> Result<bool> {
        Ok(check(&self.cfg))
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
