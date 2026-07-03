use anyhow::{anyhow, Context, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tracing::{debug, warn};

use crate::{
    config::{Action, AppConfig, OomConfig},
    plugin::{CheckState, Plugin, PluginStatus, TickOutcome},
    util,
};

pub fn journalctl_exists() -> bool {
    util::command_exists("/usr/bin/journalctl")
}

pub fn spawn_watcher(patterns: Vec<String>, oom_tx: mpsc::Sender<()>) -> Result<Child> {
    let mut child = Command::new("/usr/bin/journalctl")
        .args(["-kf", "-n", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning journalctl for OOM watcher")?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("journalctl stdout missing"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("journalctl stderr missing"))?;

    let patterns: Vec<String> = patterns.into_iter().map(|p| p.to_lowercase()).collect();

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            debug!(target = "oom", "journalctl stderr: {}", line);
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stdout);

        for line in reader.lines().map_while(Result::ok) {
            let lower = line.to_lowercase();

            if patterns.iter().any(|p| lower.contains(p)) {
                warn!(target = "oom", "OOM pattern matched: {}", line);
                let _ = oom_tx.send(());
            }
        }
    });

    Ok(child)
}

pub struct OomPlugin {
    cfg: OomConfig,
    interval: Duration,
    state: CheckState,
    child: Option<Child>,
    signature: Option<Vec<String>>,
    tx: mpsc::Sender<()>,
    rx: mpsc::Receiver<()>,
}

impl OomPlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        let (tx, rx) = mpsc::channel::<()>();

        Self {
            cfg: cfg.oom.clone(),
            interval: cfg.global.tick,
            state: CheckState::default(),
            child: None,
            signature: None,
            tx,
            rx,
        }
    }

    fn stop_watcher(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        self.signature = None;
    }

    fn ensure_watcher(&mut self) -> Result<()> {
        let desired_signature = self.cfg.patterns.clone();

        let needs_start = self.child.is_none()
            || self
                .signature
                .as_ref()
                .map(|sig| sig != &desired_signature)
                .unwrap_or(true);

        if !needs_start {
            return Ok(());
        }

        if self.child.is_some() {
            warn!("OOM pattern config changed; restarting journalctl watcher");
            self.stop_watcher();
        }

        self.child = Some(spawn_watcher(self.cfg.patterns.clone(), self.tx.clone())?);
        self.signature = Some(desired_signature);

        Ok(())
    }
}

impl Drop for OomPlugin {
    fn drop(&mut self) {
        self.stop_watcher();
    }
}

impl Plugin for OomPlugin {
    fn id(&self) -> &'static str {
        "oom"
    }

    fn name(&self) -> &'static str {
        "OOM"
    }

    fn description(&self) -> &'static str {
        "Watches journald for OOM messages"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    fn fail_limit(&self) -> u32 {
        1
    }

    fn failure_action(&self) -> Action {
        Action::Reboot
    }

    fn failure_reason(&self) -> &'static str {
        "OOM detected in journal"
    }

    fn success_message(&self) -> &'static str {
        "OOM watcher healthy"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        let old_patterns = self.cfg.patterns.clone();

        self.cfg = cfg.oom.clone();
        self.interval = cfg.global.tick;

        if self.signature.as_ref() != Some(&old_patterns) {
            self.signature = None;
        }
    }

    fn probe(&mut self, _rt: &Runtime) -> Result<bool> {
        Ok(journalctl_exists() && !self.cfg.patterns.is_empty())
    }

    fn status(&mut self, _rt: &Runtime) -> PluginStatus {
        if !self.enabled() {
            return PluginStatus::disabled(self.id(), "disabled");
        }

        if !journalctl_exists() {
            return PluginStatus::failed(self.id(), "/usr/bin/journalctl not found");
        }

        if self.cfg.patterns.is_empty() {
            return PluginStatus::failed(self.id(), "no OOM patterns configured");
        }

        PluginStatus::healthy(
            self.id(),
            format!(
                "journalctl watcher configured, {} pattern(s)",
                self.cfg.patterns.len()
            ),
        )
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!(
                    "journalctl available, {} pattern(s)",
                    self.cfg.patterns.len()
                ),
            ),
            Ok(false) => PluginStatus::failed(
                self.id(),
                "/usr/bin/journalctl unavailable or no patterns configured",
            ),
            Err(e) => PluginStatus::failed(self.id(), format!("error: {:#}", e)),
        }
    }

    fn tick(&mut self, _rt: &Runtime, now: Instant) -> TickOutcome {
        if !self.enabled() {
            self.state.reset_disabled(now);
            self.stop_watcher();
            return TickOutcome::Idle;
        }

        if !self.state.due(now, self.interval()) {
            return TickOutcome::Idle;
        }

        if let Err(e) = self.ensure_watcher() {
            return TickOutcome::Fatal {
                plugin: self.id(),
                error: format!("{:#}", e),
            };
        }

        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return TickOutcome::Fatal {
                        plugin: self.id(),
                        error: format!("journalctl watcher exited with status {}", status),
                    };
                }
                Ok(None) => {}
                Err(e) => {
                    return TickOutcome::Fatal {
                        plugin: self.id(),
                        error: format!("checking journalctl watcher status failed: {:#}", e),
                    };
                }
            }
        }

        let mut detected = false;

        while self.rx.try_recv().is_ok() {
            detected = true;
        }

        if detected {
            TickOutcome::Failure {
                plugin: self.id(),
                failures: 1,
                limit: 1,
                error: None,
                action: Some(Action::Reboot),
                reason: self.failure_reason(),
            }
        } else {
            TickOutcome::Idle
        }
    }
}
