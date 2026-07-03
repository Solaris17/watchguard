use anyhow::{anyhow, Context, Result};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tracing::{error, info, warn};

use crate::config::{Action, ActionPlan, AppConfig};

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

pub fn act(
    plan: ActionPlan,
    cfg: &AppConfig,
    start_time: Instant,
    last_reboot_attempt: &mut Option<Instant>,
    reason: &str,
) {
    match plan.action {
        Action::None => {
            info!(reason, "remediation skipped: action=none");
        }

        Action::RestartService => {
            let Some(service) = plan.service.as_deref().filter(|s| !s.trim().is_empty()) else {
                error!(
                    reason,
                    "remediation invalid: action=restart_service but no service was configured"
                );
                return;
            };

            let argv = vec![
                "/usr/bin/systemctl".to_string(),
                "restart".to_string(),
                service.to_string(),
            ];

            warn!(
                reason,
                service,
                command=?argv,
                "remediation starting: restart_service"
            );

            match run_cmd(&argv) {
                Ok(()) => info!(
                    reason,
                    service,
                    command=?argv,
                    "remediation succeeded: restart_service"
                ),
                Err(e) => error!(
                    reason,
                    service,
                    command=?argv,
                    error=?e,
                    "remediation failed: restart_service"
                ),
            }
        }

        Action::RunCommand => {
            if plan.command.is_empty() {
                error!(
                    reason,
                    "remediation invalid: action=run_command but command was empty"
                );
                return;
            }

            warn!(
                reason,
                command=?plan.command,
                "remediation starting: run_command"
            );

            match run_cmd(&plan.command) {
                Ok(()) => info!(
                    reason,
                    command=?plan.command,
                    "remediation succeeded: run_command"
                ),
                Err(e) => error!(
                    reason,
                    command=?plan.command,
                    error=?e,
                    "remediation failed: run_command"
                ),
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
                    reason,
                    remaining=?remaining,
                    "remediation suppressed: reboot blocked by boot grace"
                );
                return;
            }

            if let Some(remaining) = cooldown_remaining(*last_reboot_attempt, cfg) {
                warn!(
                    reason,
                    remaining=?remaining,
                    "remediation suppressed: reboot blocked by cooldown"
                );
                return;
            }

            warn!(
                reason,
                command=?cfg.commands.reboot,
                "remediation starting: reboot"
            );

            *last_reboot_attempt = Some(Instant::now());

            match run_cmd(&cfg.commands.reboot) {
                Ok(()) => info!(
                    reason,
                    command=?cfg.commands.reboot,
                    "remediation command accepted: reboot"
                ),
                Err(e) => error!(
                    reason,
                    command=?cfg.commands.reboot,
                    error=?e,
                    "remediation failed: reboot command"
                ),
            }
        }
    }
}
