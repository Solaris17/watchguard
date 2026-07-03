use anyhow::{anyhow, Context, Result};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tracing::{error, info, warn};

use crate::{
    config::{Action, ActionPlan, AppConfig},
    state,
};

pub fn run_cmd(argv: &[String]) -> Result<()> {
    let (prog, args) = argv.split_first().ok_or_else(|| anyhow!("empty command"))?;

    let status = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("running {}", prog))?;

    if !status.success() {
        return Err(anyhow!("command failed: {:?} status={}", argv, status));
    }

    Ok(())
}

pub fn in_boot_grace(start_time: Instant, cfg: &AppConfig) -> bool {
    start_time.elapsed() < cfg.global.boot_grace_period
}

pub fn cooldown_remaining(last: Option<Instant>, cfg: &AppConfig) -> Option<Duration> {
    let last = last?;
    let elapsed = last.elapsed();

    if elapsed < cfg.global.reboot_cooldown {
        Some(cfg.global.reboot_cooldown - elapsed)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn act(
    plugin: &str,
    plan: ActionPlan,
    cfg: &AppConfig,
    start_time: Instant,
    last_reboot_attempt: &mut Option<Instant>,
    reason: &str,
    details: Option<&str>,
    failures: u32,
    limit: u32,
) {
    let action = action_name(plan.action);

    match plan.action {
        Action::None => {
            info!(
                plugin,
                action,
                reason,
                details,
                failures,
                limit,
                "remediation skipped: plugin={} action=none reason={} failures={}/{} details={:?}",
                plugin,
                reason,
                failures,
                limit,
                details
            );

            record_state(
                plugin,
                action,
                "skipped",
                reason,
                "configured action is none; no remediation taken",
                details,
                &[],
                None,
                Some(failures),
                Some(limit),
            );
        }

        Action::RestartService => {
            let Some(service) = plan.service.as_deref().filter(|s| !s.trim().is_empty()) else {
                error!(
                    plugin,
                    action,
                    reason,
                    details,
                    "remediation invalid: plugin={} action=restart_service reason={} missing service details={:?}",
                    plugin,
                    reason,
                    details
                );

                record_state(
                    plugin,
                    action,
                    "invalid",
                    reason,
                    "restart_service action was missing service",
                    details,
                    &[],
                    None,
                    Some(failures),
                    Some(limit),
                );

                return;
            };

            let argv = vec![
                "/usr/bin/systemctl".to_string(),
                "restart".to_string(),
                service.to_string(),
            ];

            warn!(
                plugin,
                action,
                reason,
                details,
                service,
                failures,
                limit,
                command=?argv,
                "remediation starting: plugin={} action=restart_service service={} failures={}/{} reason={} command={:?} details={:?}",
                plugin,
                service,
                failures,
                limit,
                reason,
                argv,
                details
            );

            record_state(
                plugin,
                action,
                "started",
                reason,
                format!("restarting service {}", service),
                details,
                &argv,
                Some(service),
                Some(failures),
                Some(limit),
            );

            match run_cmd(&argv) {
                Ok(()) => {
                    info!(
                        plugin,
                        action,
                        reason,
                        service,
                        command=?argv,
                        "remediation succeeded: plugin={} action=restart_service service={} reason={} command={:?}",
                        plugin,
                        service,
                        reason,
                        argv
                    );

                    record_state(
                        plugin,
                        action,
                        "succeeded",
                        reason,
                        format!("service restart completed for {}", service),
                        details,
                        &argv,
                        Some(service),
                        Some(failures),
                        Some(limit),
                    );
                }
                Err(e) => {
                    error!(
                        plugin,
                        action,
                        reason,
                        service,
                        command=?argv,
                        error=?e,
                        "remediation failed: plugin={} action=restart_service service={} reason={} command={:?} error={:?}",
                        plugin,
                        service,
                        reason,
                        argv,
                        e
                    );

                    record_state(
                        plugin,
                        action,
                        "failed",
                        reason,
                        format!("service restart failed for {}: {:#}", service, e),
                        details,
                        &argv,
                        Some(service),
                        Some(failures),
                        Some(limit),
                    );
                }
            }
        }

        Action::RunCommand => {
            if plan.command.is_empty() {
                error!(
                    plugin,
                    action,
                    reason,
                    details,
                    "remediation invalid: plugin={} action=run_command reason={} empty command details={:?}",
                    plugin,
                    reason,
                    details
                );

                record_state(
                    plugin,
                    action,
                    "invalid",
                    reason,
                    "run_command action had an empty command",
                    details,
                    &[],
                    None,
                    Some(failures),
                    Some(limit),
                );

                return;
            }

            warn!(
                plugin,
                action,
                reason,
                details,
                failures,
                limit,
                command=?plan.command,
                "remediation starting: plugin={} action=run_command failures={}/{} reason={} command={:?} details={:?}",
                plugin,
                failures,
                limit,
                reason,
                plan.command,
                details
            );

            record_state(
                plugin,
                action,
                "started",
                reason,
                "running configured command",
                details,
                &plan.command,
                None,
                Some(failures),
                Some(limit),
            );

            match run_cmd(&plan.command) {
                Ok(()) => {
                    info!(
                        plugin,
                        action,
                        reason,
                        command=?plan.command,
                        "remediation succeeded: plugin={} action=run_command reason={} command={:?}",
                        plugin,
                        reason,
                        plan.command
                    );

                    record_state(
                        plugin,
                        action,
                        "succeeded",
                        reason,
                        "configured command completed successfully",
                        details,
                        &plan.command,
                        None,
                        Some(failures),
                        Some(limit),
                    );
                }
                Err(e) => {
                    error!(
                        plugin,
                        action,
                        reason,
                        command=?plan.command,
                        error=?e,
                        "remediation failed: plugin={} action=run_command reason={} command={:?} error={:?}",
                        plugin,
                        reason,
                        plan.command,
                        e
                    );

                    record_state(
                        plugin,
                        action,
                        "failed",
                        reason,
                        format!("configured command failed: {:#}", e),
                        details,
                        &plan.command,
                        None,
                        Some(failures),
                        Some(limit),
                    );
                }
            }
        }

        Action::Reboot => {
            if in_boot_grace(start_time, cfg) {
                let remaining = cfg
                    .global
                    .boot_grace_period
                    .checked_sub(start_time.elapsed())
                    .unwrap_or_default();

                warn!(
                    plugin,
                    action,
                    reason,
                    details,
                    failures,
                    limit,
                    remaining=?remaining,
                    "remediation suppressed: plugin={} action=reboot reason={} blocked_by=boot_grace remaining={:?} failures={}/{} details={:?}",
                    plugin,
                    reason,
                    remaining,
                    failures,
                    limit,
                    details
                );

                record_state(
                    plugin,
                    action,
                    "suppressed",
                    reason,
                    format!("reboot blocked by boot grace; remaining {:?}", remaining),
                    details,
                    &cfg.commands.reboot,
                    None,
                    Some(failures),
                    Some(limit),
                );

                return;
            }

            if let Some(remaining) = cooldown_remaining(*last_reboot_attempt, cfg) {
                warn!(
                    plugin,
                    action,
                    reason,
                    details,
                    failures,
                    limit,
                    remaining=?remaining,
                    "remediation suppressed: plugin={} action=reboot reason={} blocked_by=cooldown remaining={:?} failures={}/{} details={:?}",
                    plugin,
                    reason,
                    remaining,
                    failures,
                    limit,
                    details
                );

                record_state(
                    plugin,
                    action,
                    "suppressed",
                    reason,
                    format!("reboot blocked by cooldown; remaining {:?}", remaining),
                    details,
                    &cfg.commands.reboot,
                    None,
                    Some(failures),
                    Some(limit),
                );

                return;
            }

            warn!(
                plugin,
                action,
                reason,
                details,
                failures,
                limit,
                command=?cfg.commands.reboot,
                "remediation starting: plugin={} action=reboot reason={} failures={}/{} command={:?} details={:?}",
                plugin,
                reason,
                failures,
                limit,
                cfg.commands.reboot,
                details
            );

            record_state(
                plugin,
                action,
                "started",
                reason,
                "reboot command is about to be executed",
                details,
                &cfg.commands.reboot,
                None,
                Some(failures),
                Some(limit),
            );

            *last_reboot_attempt = Some(Instant::now());

            match run_cmd(&cfg.commands.reboot) {
                Ok(()) => {
                    info!(
                        plugin,
                        action,
                        reason,
                        command=?cfg.commands.reboot,
                        "remediation command accepted: plugin={} action=reboot reason={} command={:?}",
                        plugin,
                        reason,
                        cfg.commands.reboot
                    );

                    record_state(
                        plugin,
                        action,
                        "accepted",
                        reason,
                        "reboot command was accepted",
                        details,
                        &cfg.commands.reboot,
                        None,
                        Some(failures),
                        Some(limit),
                    );
                }
                Err(e) => {
                    error!(
                        plugin,
                        action,
                        reason,
                        command=?cfg.commands.reboot,
                        error=?e,
                        "remediation failed: plugin={} action=reboot reason={} command={:?} error={:?}",
                        plugin,
                        reason,
                        cfg.commands.reboot,
                        e
                    );

                    record_state(
                        plugin,
                        action,
                        "failed",
                        reason,
                        format!("reboot command failed: {:#}", e),
                        details,
                        &cfg.commands.reboot,
                        None,
                        Some(failures),
                        Some(limit),
                    );
                }
            }
        }
    }
}

fn record_state(
    plugin: &str,
    action: &str,
    status: &str,
    reason: &str,
    message: impl Into<String>,
    details: Option<&str>,
    command: &[String],
    service: Option<&str>,
    failures: Option<u32>,
    limit: Option<u32>,
) {
    if let Err(e) = state::record_event(
        plugin, action, status, reason, message, details, command, service, failures, limit,
    ) {
        error!(
            plugin,
            action,
            status,
            error=?e,
            "failed to write watchguard state DB: plugin={} action={} status={} error={:?}",
            plugin,
            action,
            status,
            e
        );
    }
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::None => "none",
        Action::RestartService => "restart_service",
        Action::RunCommand => "run_command",
        Action::Reboot => "reboot",
    }
}
