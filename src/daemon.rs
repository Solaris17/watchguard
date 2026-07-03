use anyhow::{Context, Result};
use std::{
    process::Child,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

use crate::{
    actions,
    config::{self, Action, AppConfig},
    plugins,
};

type EnabledFn = fn(&AppConfig) -> bool;
type IntervalFn = fn(&AppConfig) -> Duration;
type LimitFn = fn(&AppConfig) -> u32;
type ActionFn = fn(&AppConfig) -> Action;
type CheckFn = fn(&AppConfig, &Runtime) -> Result<bool>;

struct PeriodicCheck {
    id: &'static str,
    success_message: &'static str,
    failure_reason: &'static str,
    next: Instant,
    fails: u32,
    enabled: EnabledFn,
    interval: IntervalFn,
    limit: LimitFn,
    action: ActionFn,
    check: CheckFn,
}

impl PeriodicCheck {
    fn run_if_due(
        &mut self,
        cfg: &AppConfig,
        rt: &Runtime,
        now: Instant,
        start_time: Instant,
        last_reboot_attempt: &mut Option<Instant>,
    ) {
        if !(self.enabled)(cfg) {
            self.fails = 0;
            self.next = now;
            return;
        }

        if now < self.next {
            return;
        }

        self.next = now + (self.interval)(cfg);

        match (self.check)(cfg, rt) {
            Ok(true) => {
                if self.fails != 0 {
                    info!(
                        plugin = self.id,
                        fails = self.fails,
                        "{}",
                        self.success_message
                    );
                }
                self.fails = 0;
            }
            Ok(false) => self.record_failure(cfg, start_time, last_reboot_attempt, None),
            Err(e) => self.record_failure(cfg, start_time, last_reboot_attempt, Some(e)),
        }
    }

    fn record_failure(
        &mut self,
        cfg: &AppConfig,
        start_time: Instant,
        last_reboot_attempt: &mut Option<Instant>,
        error: Option<anyhow::Error>,
    ) {
        self.fails += 1;

        if let Some(error) = error {
            warn!(
                plugin=self.id,
                error=?error,
                fails=self.fails,
                limit=(self.limit)(cfg),
                "plugin check error"
            );
        } else {
            warn!(
                plugin = self.id,
                fails = self.fails,
                limit = (self.limit)(cfg),
                "plugin check failed"
            );
        }

        if self.fails >= (self.limit)(cfg) {
            actions::act(
                (self.action)(cfg),
                cfg,
                start_time,
                last_reboot_attempt,
                self.failure_reason,
            );
            self.fails = 0;
        }
    }
}

fn registered_checks() -> Vec<PeriodicCheck> {
    vec![
        PeriodicCheck {
            id: "ssh-service",
            success_message: "SSH service recovered",
            failure_reason: "SSH service failure limit exceeded",
            next: Instant::now(),
            fails: 0,
            enabled: |cfg| cfg.ssh.enabled && cfg.ssh.service_check_enabled,
            interval: |cfg| cfg.ssh.service_check_interval,
            limit: |cfg| cfg.ssh.service_fail_limit,
            action: |cfg| cfg.ssh.service_failure_action,
            check: |cfg, rt| rt.block_on(plugins::ssh::systemd_unit_is_active(&cfg.ssh.service)),
        },
        PeriodicCheck {
            id: "ssh-targets",
            success_message: "SSH target reachability recovered",
            failure_reason: "SSH target reachability failure limit exceeded",
            next: Instant::now(),
            fails: 0,
            enabled: |cfg| cfg.ssh.enabled && cfg.ssh.target_check_enabled,
            interval: |cfg| cfg.ssh.ssh_check_interval,
            limit: |cfg| cfg.ssh.ssh_fail_limit,
            action: |cfg| cfg.ssh.ssh_failure_action,
            check: |cfg, _rt| Ok(plugins::ssh::targets_ok(&cfg.ssh)),
        },
        PeriodicCheck {
            id: "network",
            success_message: "Network reachability recovered",
            failure_reason: "Network failure limit exceeded",
            next: Instant::now(),
            fails: 0,
            enabled: |cfg| cfg.network.enabled,
            interval: |cfg| cfg.network.check_interval,
            limit: |cfg| cfg.network.fail_limit,
            action: |cfg| cfg.network.failure_action,
            check: |cfg, _rt| Ok(plugins::network::check(&cfg.network)),
        },
        PeriodicCheck {
            id: "dns",
            success_message: "DNS recovered",
            failure_reason: "DNS failure limit exceeded",
            next: Instant::now(),
            fails: 0,
            enabled: |cfg| cfg.dns.enabled,
            interval: |cfg| cfg.dns.check_interval,
            limit: |cfg| cfg.dns.fail_limit,
            action: |cfg| cfg.dns.failure_action,
            check: |cfg, _rt| Ok(plugins::dns::check(&cfg.dns)),
        },
    ]
}

pub fn daemon_loop(config_path: &str) -> Result<()> {
    let initial_cfg = config::load_config(config_path)?;
    init_logging(&initial_cfg.global.log_level)?;

    info!("watchguard daemon starting; config={}", config_path);

    let rt = Runtime::new().context("creating Tokio runtime")?;

    let start_time = Instant::now();
    let mut last_reboot_attempt: Option<Instant> = None;

    let (oom_tx, oom_rx) = mpsc::channel::<()>();
    let mut oom_child: Option<Child> = None;
    let mut oom_signature: Option<Vec<String>> = None;

    let mut checks = registered_checks();

    loop {
        let cfg = match config::load_config(config_path) {
            Ok(v) => v,
            Err(e) => {
                error!(error=?e, "config reload failed; exiting so systemd restarts watchguard");
                std::process::exit(2);
            }
        };

        sync_oom_watcher(&cfg, &oom_tx, &mut oom_child, &mut oom_signature)?;

        if cfg.oom.enabled {
            if let Some(child) = oom_child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    error!(
                        status=?status,
                        "journalctl OOM watcher exited; exiting so systemd restarts watchguard"
                    );
                    std::process::exit(2);
                }
            }
        }

        while oom_rx.try_recv().is_ok() {
            actions::act(
                Action::Reboot,
                &cfg,
                start_time,
                &mut last_reboot_attempt,
                "OOM detected in journal",
            );
        }

        let now = Instant::now();

        for check in checks.iter_mut() {
            check.run_if_due(&cfg, &rt, now, start_time, &mut last_reboot_attempt);
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

fn sync_oom_watcher(
    cfg: &AppConfig,
    oom_tx: &mpsc::Sender<()>,
    oom_child: &mut Option<Child>,
    oom_signature: &mut Option<Vec<String>>,
) -> Result<()> {
    if !cfg.oom.enabled {
        if let Some(mut child) = oom_child.take() {
            warn!("OOM plugin disabled; stopping journalctl watcher");
            let _ = child.kill();
            let _ = child.wait();
        }

        *oom_signature = None;
        return Ok(());
    }

    let desired_signature = cfg.oom.patterns.clone();

    let needs_start = oom_child.is_none()
        || oom_signature
            .as_ref()
            .map(|sig| sig != &desired_signature)
            .unwrap_or(true);

    if needs_start {
        if let Some(mut child) = oom_child.take() {
            warn!("OOM pattern config changed; restarting journalctl watcher");
            let _ = child.kill();
            let _ = child.wait();
        }

        info!("starting OOM watcher via journalctl -kf -n0");
        *oom_child = Some(plugins::oom::spawn_watcher(
            cfg.oom.patterns.clone(),
            oom_tx.clone(),
        )?);
        *oom_signature = Some(desired_signature);
    }

    Ok(())
}
