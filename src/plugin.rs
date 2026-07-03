use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::config::{Action, ActionPlan, AppConfig, EscalationStep};

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
        action: Option<ActionPlan>,
        reason: &'static str,
        details: Option<String>,
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
    last_action_at_failures: u32,
}

impl Default for CheckState {
    fn default() -> Self {
        Self {
            next_run: Instant::now(),
            failures: 0,
            last_action_at_failures: 0,
        }
    }
}

impl CheckState {
    pub fn reset_disabled(&mut self, now: Instant) {
        self.failures = 0;
        self.last_action_at_failures = 0;
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
        escalation_steps: &[EscalationStep],
        failure_reason: &'static str,
        success_message: &'static str,
        result: Result<bool>,
    ) -> TickOutcome {
        let plan = normalize_escalation_steps(escalation_steps);

        match result {
            Ok(true) => {
                if self.failures == 0 {
                    TickOutcome::Idle
                } else {
                    let failures = self.failures;
                    self.failures = 0;
                    self.last_action_at_failures = 0;

                    TickOutcome::Recovered {
                        plugin,
                        failures,
                        message: success_message,
                    }
                }
            }
            Ok(false) => self.record_failure(plugin, &plan, failure_reason, None),
            Err(e) => self.record_failure(plugin, &plan, failure_reason, Some(format!("{:#}", e))),
        }
    }

    fn record_failure(
        &mut self,
        plugin: &'static str,
        plan: &[EscalationStep],
        failure_reason: &'static str,
        error: Option<String>,
    ) -> TickOutcome {
        self.failures += 1;
        let failures = self.failures;
        let next_limit = next_action_threshold(plan, self.last_action_at_failures)
            .unwrap_or_else(|| repeated_final_threshold(plan, self.last_action_at_failures));

        if let Some(step) = due_step(plan, failures, self.last_action_at_failures) {
            let action_plan = step.action_plan();
            self.last_action_at_failures = failures;

            return TickOutcome::Failure {
                plugin,
                failures,
                limit: step.after_failures,
                error,
                action: Some(action_plan),
                reason: failure_reason,
                details: None,
            };
        }

        TickOutcome::Failure {
            plugin,
            failures,
            limit: next_limit,
            error,
            action: None,
            reason: failure_reason,
            details: None,
        }
    }
}

pub fn normalize_escalation_steps(steps: &[EscalationStep]) -> Vec<EscalationStep> {
    let mut plan = steps
        .iter()
        .cloned()
        .filter(|step| step.after_failures > 0)
        .collect::<Vec<_>>();

    plan.sort_by_key(|step| step.after_failures);
    plan.dedup_by_key(|step| step.after_failures);

    if plan.is_empty() {
        plan.push(EscalationStep::new(1, Action::None));
    }

    plan
}

fn due_step(
    plan: &[EscalationStep],
    failures: u32,
    last_action_at_failures: u32,
) -> Option<EscalationStep> {
    if let Some(step) = plan
        .iter()
        .filter(|step| step.after_failures > last_action_at_failures)
        .filter(|step| failures >= step.after_failures)
        .max_by_key(|step| step.after_failures)
    {
        return Some(step.clone());
    }

    let last = plan.last()?;

    if last_action_at_failures >= last.after_failures
        && failures >= last_action_at_failures.saturating_add(last.after_failures)
    {
        return Some(last.clone());
    }

    None
}

fn next_action_threshold(plan: &[EscalationStep], last_action_at_failures: u32) -> Option<u32> {
    plan.iter()
        .filter(|step| step.after_failures > last_action_at_failures)
        .map(|step| step.after_failures)
        .min()
}

fn repeated_final_threshold(plan: &[EscalationStep], last_action_at_failures: u32) -> u32 {
    let repeat = plan
        .last()
        .map(|step| step.after_failures.max(1))
        .unwrap_or(1);

    last_action_at_failures.saturating_add(repeat).max(repeat)
}

pub fn resolved_escalation_steps_for(plugin: &dyn Plugin) -> Vec<EscalationStep> {
    normalize_escalation_steps(&plugin.escalation_steps())
}

pub trait Plugin {
    fn id(&self) -> &'static str;

    fn name(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn enabled(&self) -> bool;

    fn interval(&self) -> Duration;

    fn escalation_steps(&self) -> Vec<EscalationStep> {
        Vec::new()
    }

    fn remediation_mode(&self) -> &'static str {
        "escalation"
    }

    fn remediation_summary(&self) -> Option<String> {
        None
    }

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
