use anyhow::{Context, Result};
use std::{thread, time::Instant};
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

use crate::{actions, config, plugin::TickOutcome, registry};

pub fn daemon_loop(config_path: &str) -> Result<()> {
    let initial_cfg = config::load_config(config_path)?;
    init_logging(&initial_cfg.global.log_level)?;

    info!(
        config = config_path,
        log_level = initial_cfg.global.log_level,
        tick = ?initial_cfg.global.tick,
        boot_grace_period = ?initial_cfg.global.boot_grace_period,
        reboot_cooldown = ?initial_cfg.global.reboot_cooldown,
        "watchguard daemon starting"
    );

    let rt = Runtime::new().context("creating Tokio runtime")?;
    let start_time = Instant::now();
    let mut last_reboot_attempt: Option<Instant> = None;
    let mut plugins = registry::build_plugins(&initial_cfg);

    for plugin in plugins.iter_mut() {
        plugin.update_config(&initial_cfg);
        info!(
            plugin = plugin.id(),
            enabled = plugin.enabled(),
            interval = ?plugin.interval(),
            "plugin registered"
        );
    }

    loop {
        let cfg = match config::load_config(config_path) {
            Ok(v) => v,
            Err(e) => {
                error!(error=?e, "config reload failed; exiting so systemd restarts watchguard");
                std::process::exit(2);
            }
        };

        for plugin in plugins.iter_mut() {
            plugin.update_config(&cfg);
        }

        let now = Instant::now();

        for plugin in plugins.iter_mut() {
            match plugin.tick(&rt, now) {
                TickOutcome::Idle => {}

                TickOutcome::Recovered {
                    plugin,
                    failures,
                    message,
                } => {
                    info!(plugin, failures, "plugin recovered: {}", message);
                }

                TickOutcome::Failure {
                    plugin,
                    failures,
                    limit,
                    error,
                    action,
                    reason,
                } => {
                    if let Some(error) = error {
                        warn!(
                            plugin,
                            failures,
                            limit,
                            error,
                            reason,
                            action = ?action,
                            "plugin check failed"
                        );
                    } else {
                        warn!(
                            plugin,
                            failures,
                            limit,
                            reason,
                            action = ?action,
                            "plugin check failed"
                        );
                    }

                    if let Some(action) = action {
                        actions::act(action, &cfg, start_time, &mut last_reboot_attempt, reason);
                    }
                }

                TickOutcome::Fatal { plugin, error } => {
                    error!(
                        plugin,
                        error, "fatal plugin error; exiting so systemd restarts watchguard"
                    );
                    std::process::exit(2);
                }
            }
        }

        thread::sleep(cfg.global.tick);
    }
}

fn init_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let filter = match level {
        "trace" => "trace",
        "debug" => "debug",
        "info" => "info",
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    };

    let journald = tracing_journald::layer().context("creating journald logging layer")?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(journald)
        .init();

    Ok(())
}
