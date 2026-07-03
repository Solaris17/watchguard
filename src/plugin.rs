use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::config::{Action, AppConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Healthy,
    Warning,
    Failed,
    Disabled,
    Skipped,
}

impl Health {
    pub fn icon(self) -> &'static str {
        match self {
            Health::Healthy => "✅",
            Health::Warning => "⚠️ ",
            Health::Failed => "❌",
            Health::Disabled => "❌",
            Health::Skipped => "⏭️ ",
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(self, Health::Failed)
    }

    pub fn is_warning(self) -> bool {
        matches!(self, Health::Warning)
    }
}

#[derive(Debug, Clone)]
pub struct PluginStatus {
    pub id: &'static str,
    pub health: Health,
    pub message: String,
}

impl PluginStatus {
    pub fn new(id: &'static str, health: Health, message: impl Into<String>) -> Self {
        Self {
            id,
            health,
            message: message.into(),
        }
    }

    pub fn healthy(id: &'static str, message: impl Into<String>) -> Self {
        Self::new(id, Health::Healthy, message)
    }

    pub fn warning(id: &'static str, message: impl Into<String>) -> Self {
        Self::new(id, Health::Warning, message)
    }

    pub fn failed(id: &'static str, message: impl Into<String>) -> Self {
        Self::new(id, Health::Failed, message)
    }

    pub fn disabled(id: &'static str, message: impl Into<String>) -> Self {
        Self::new(id, Health::Disabled, message)
    }

    pub fn skipped(id: &'static str, message: impl Into<String>) -> Self {
        Self::new(id, Health::Skipped, message)
    }
}

pub fn format_status_line(status: &PluginStatus) -> String {
    format!(
        "{} {:<12} {}",
        status.health.icon(),
        status.id,
        status.message
    )
}

#[derive(Debug)]
pub enum TickOutcome {
    Idle,
    Recovered {
        plugin: &'static str,
        failures: u32,
        message: &'static str,
    },
    Failure {
        plugin: &'static str,
        failures: u32,
        limit: u32,
        error: Option<String>,
        action: Option<Action>,
        reason: &'static str,
    },
    Fatal {
        plugin: &'static str,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct CheckState {
    next_run: Instant,
    failures: u32,
}

impl Default for CheckState {
    fn default() -> Self {
        Self {
            next_run: Instant::now(),
            failures: 0,
        }
    }
}

impl CheckState {
    pub fn reset_disabled(&mut self, now: Instant) {
        self.failures = 0;
        self.next_run = now;
    }

    pub fn due(&mut self, now: Instant, interval: Duration) -> bool {
        if now < self.next_run {
            return false;
        }

        self.next_run = now + interval;
        true
    }

    pub fn record(
        &mut self,
        plugin: &'static str,
        fail_limit: u32,
        action: Action,
        failure_reason: &'static str,
        success_message: &'static str,
        result: Result<bool>,
    ) -> TickOutcome {
        let fail_limit = fail_limit.max(1);

        match result {
            Ok(true) => {
                if self.failures == 0 {
                    TickOutcome::Idle
                } else {
                    let failures = self.failures;
                    self.failures = 0;

                    TickOutcome::Recovered {
                        plugin,
                        failures,
                        message: success_message,
                    }
                }
            }
            Ok(false) => self.record_failure(plugin, fail_limit, action, failure_reason, None),
            Err(e) => self.record_failure(
                plugin,
                fail_limit,
                action,
                failure_reason,
                Some(format!("{:#}", e)),
            ),
        }
    }

    fn record_failure(
        &mut self,
        plugin: &'static str,
        fail_limit: u32,
        action: Action,
        failure_reason: &'static str,
        error: Option<String>,
    ) -> TickOutcome {
        self.failures += 1;
        let failures = self.failures;

        if self.failures >= fail_limit {
            self.failures = 0;

            TickOutcome::Failure {
                plugin,
                failures,
                limit: fail_limit,
                error,
                action: Some(action),
                reason: failure_reason,
            }
        } else {
            TickOutcome::Failure {
                plugin,
                failures,
                limit: fail_limit,
                error,
                action: None,
                reason: failure_reason,
            }
        }
    }
}

pub trait Plugin {
    fn id(&self) -> &'static str;

    fn name(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn enabled(&self) -> bool;

    fn interval(&self) -> Duration;

    fn fail_limit(&self) -> u32;

    fn failure_action(&self) -> Action;

    fn failure_reason(&self) -> &'static str;

    fn success_message(&self) -> &'static str;

    fn update_config(&mut self, cfg: &AppConfig);

    fn probe(&mut self, rt: &Runtime) -> Result<bool>;

    fn status(&mut self, rt: &Runtime) -> PluginStatus;

    fn doctor(&mut self, rt: &Runtime) -> Vec<PluginStatus> {
        vec![self.status(rt)]
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(self.id(), "probe succeeded"),
            Ok(false) => PluginStatus::failed(self.id(), "probe failed"),
            Err(e) => PluginStatus::failed(self.id(), format!("error: {:#}", e)),
        }
    }

    fn tick(&mut self, rt: &Runtime, now: Instant) -> TickOutcome;
}
