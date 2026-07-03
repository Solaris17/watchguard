use anyhow::{anyhow, Context, Result};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tracing::{error, warn};

use crate::config::{Action, AppConfig};

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
    action: Action,
    cfg: &AppConfig,
    start_time: Instant,
    last_reboot_attempt: &mut Option<Instant>,
    reason: &str,
) {
    match action {
        Action::None => {
            warn!(reason, "action=none; no remediation taken");
        }
        Action::Restart => {
            warn!(reason, "action=restart ssh");
            if let Err(e) = run_cmd(&cfg.commands.restart_ssh) {
                error!(error=?e, "failed to restart SSH service");
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
                    "boot grace active; reboot suppressed"
                );
                return;
            }

            if let Some(remaining) = cooldown_remaining(*last_reboot_attempt, cfg) {
                warn!(
                    reason,
                    remaining=?remaining,
                    "reboot cooldown active; reboot suppressed"
                );
                return;
            }

            warn!(reason, "action=reboot");
            *last_reboot_attempt = Some(Instant::now());

            if let Err(e) = run_cmd(&cfg.commands.reboot) {
                error!(error=?e, "failed to run reboot command");
            }
        }
    }
}
