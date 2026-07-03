use anyhow::{Context, Result};
use std::{
    process::Child,
    sync::mpsc,
    thread,
    time::Instant,
};
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

use crate::{
    actions,
    config::{self, Action, AppConfig},
    plugins,
};

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

    let mut next_service = Instant::now();
    let mut next_ssh_targets = Instant::now();
    let mut next_network = Instant::now();
    let mut next_dns = Instant::now();

    let mut service_fails = 0_u32;
    let mut ssh_target_fails = 0_u32;
    let mut network_fails = 0_u32;
    let mut dns_fails = 0_u32;

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

        if cfg.ssh.enabled && cfg.ssh.service_check_enabled && now >= next_service {
            next_service = now + cfg.ssh.service_check_interval;

            match rt.block_on(plugins::ssh::systemd_unit_is_active(&cfg.ssh.service)) {
                Ok(true) => {
                    if service_fails != 0 {
                        info!(fails=service_fails, "SSH service recovered");
                    }
                    service_fails = 0;
                }
                Ok(false) => {
                    service_fails += 1;
                    warn!(
                        service=%cfg.ssh.service,
                        fails=service_fails,
                        limit=cfg.ssh.service_fail_limit,
                        "SSH service is not active"
                    );

                    if service_fails >= cfg.ssh.service_fail_limit {
                        actions::act(
                            cfg.ssh.service_failure_action,
                            &cfg,
                            start_time,
                            &mut last_reboot_attempt,
                            "SSH service failure limit exceeded",
                        );
                        service_fails = 0;
                    }
                }
                Err(e) => {
                    service_fails += 1;
                    warn!(
                        error=?e,
                        fails=service_fails,
                        limit=cfg.ssh.service_fail_limit,
                        "SSH service check error"
                    );

                    if service_fails >= cfg.ssh.service_fail_limit {
                        actions::act(
                            cfg.ssh.service_failure_action,
                            &cfg,
                            start_time,
                            &mut last_reboot_attempt,
                            "SSH service check error limit exceeded",
                        );
                        service_fails = 0;
                    }
                }
            }
        }

        if cfg.ssh.enabled && cfg.ssh.target_check_enabled && now >= next_ssh_targets {
            next_ssh_targets = now + cfg.ssh.ssh_check_interval;

            let ok = plugins::ssh::targets_ok(&cfg.ssh);

            if ok {
                if ssh_target_fails != 0 {
                    info!(fails=ssh_target_fails, "SSH target reachability recovered");
                }
                ssh_target_fails = 0;
            } else {
                ssh_target_fails += 1;
                warn!(
                    fails=ssh_target_fails,
                    limit=cfg.ssh.ssh_fail_limit,
                    require_all=cfg.ssh.require_all,
                    targets=?cfg.ssh.targets,
                    "SSH target reachability failed"
                );

                if ssh_target_fails >= cfg.ssh.ssh_fail_limit {
                    actions::act(
                        cfg.ssh.ssh_failure_action,
                        &cfg,
                        start_time,
                        &mut last_reboot_attempt,
                        "SSH target reachability failure limit exceeded",
                    );
                    ssh_target_fails = 0;
                }
            }
        }

        if cfg.network.enabled && now >= next_network {
            next_network = now + cfg.network.check_interval;

            let ok = plugins::network::check(&cfg.network);

            if ok {
                if network_fails != 0 {
                    info!(fails=network_fails, "network reachability recovered");
                }
                network_fails = 0;
            } else {
                network_fails += 1;
                warn!(
                    fails=network_fails,
                    limit=cfg.network.fail_limit,
                    require_all=cfg.network.require_all,
                    targets=?cfg.network.targets,
                    "network reachability failed"
                );

                if network_fails >= cfg.network.fail_limit {
                    actions::act(
                        cfg.network.failure_action,
                        &cfg,
                        start_time,
                        &mut last_reboot_attempt,
                        "network failure limit exceeded",
                    );
                    network_fails = 0;
                }
            }
        }

        if cfg.dns.enabled && now >= next_dns {
            next_dns = now + cfg.dns.check_interval;

            let ok = plugins::dns::check(&cfg.dns);

            if ok {
                if dns_fails != 0 {
                    info!(fails=dns_fails, "DNS recovered");
                }
                dns_fails = 0;
            } else {
                dns_fails += 1;
                warn!(
                    fails=dns_fails,
                    limit=cfg.dns.fail_limit,
                    server=%cfg.dns.server,
                    name=%cfg.dns.name,
                    "DNS check failed"
                );

                if dns_fails >= cfg.dns.fail_limit {
                    actions::act(
                        cfg.dns.failure_action,
                        &cfg,
                        start_time,
                        &mut last_reboot_attempt,
                        "DNS failure limit exceeded",
                    );
                    dns_fails = 0;
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
