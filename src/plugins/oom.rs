
use anyhow::{anyhow, Context, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tracing::{debug, info, warn};

use crate::{
    config::{Action, ActionPlan, AppConfig, OomConfig},
    plugin::{CheckState, Plugin, PluginStatus, TickOutcome},
    util,
};

#[derive(Debug, Clone)]
pub struct OomEvent {
    pub line: String,
}

pub fn journalctl_exists() -> bool {
    util::command_exists("/usr/bin/journalctl")
}

pub fn spawn_watcher(
    patterns: Vec<String>,
    debounce: Duration,
    oom_tx: mpsc::Sender<OomEvent>,
) -> Result<Child> {
    info!(
        pattern_count = patterns.len(),
        debounce = ?debounce,
"starting OOM journal watcher: command='journalctl -kf -n 0' pattern_count={} debounce={:?}",
        patterns.len(),
        debounce
    );

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
        let mut last_sent: Option<Instant> = None;

        for line in reader.lines().map_while(Result::ok) {
            let lower = line.to_lowercase();

            if patterns.iter().any(|p| lower.contains(p)) {
                let now = Instant::now();

                if last_sent
                    .map(|last| now.duration_since(last) < debounce)
                    .unwrap_or(false)
                {
                    debug!(
                        target = "oom",
                        journal_line = %line,
                        debounce = ?debounce,
"duplicate OOM pattern suppressed by debounce: debounce={:?} line={}",
                        debounce,
                        line
                    );
                    continue;
                }

                last_sent = Some(now);

                warn!(
                    target = "oom",
                    journal_line = %line,
"OOM kernel pattern matched: line={}",
                    line
                );

                let _ = oom_tx.send(OomEvent { line });
            }
        }

        warn!(target = "oom", "OOM journal watcher stdout ended");
    });

    Ok(child)
}

pub struct OomPlugin {
    cfg: OomConfig,
    interval: Duration,
    state: CheckState,
    child: Option<Child>,
    signature: Option<(Vec<String>, Duration)>,
    tx: mpsc::Sender<OomEvent>,
    rx: mpsc::Receiver<OomEvent>,
}

impl OomPlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        let (tx, rx) = mpsc::channel::<OomEvent>();

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
            info!("stopping OOM journal watcher");
            let _ = child.kill();
            let _ = child.wait();
        }

        self.signature = None;
    }

    fn desired_signature(&self) -> (Vec<String>, Duration) {
        (self.cfg.patterns.clone(), self.cfg.debounce)
    }

    fn ensure_watcher(&mut self) -> Result<()> {
        let desired_signature = self.desired_signature();

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
            warn!("OOM pattern or debounce config changed; restarting journalctl watcher");
            self.stop_watcher();
        }

        self.child = Some(spawn_watcher(
            self.cfg.patterns.clone(),
            self.cfg.debounce,
            self.tx.clone(),
        )?);
        self.signature = Some(desired_signature);

        info!(
            pattern_count = self.cfg.patterns.len(),
            debounce = ?self.cfg.debounce,
"OOM journal watcher active: pattern_count={} debounce={:?}",
            self.cfg.patterns.len(),
            self.cfg.debounce
        );

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
        "Watches kernel journald messages for OOM events and immediately requests reboot"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    fn remediation_mode(&self) -> &'static str {
        "event-immediate"
    }

    fn remediation_summary(&self) -> Option<String> {
        Some(format!(
            "reboot immediately on first matched kernel OOM journal event; debounce {:?}",
            self.cfg.debounce
        ))
    }

    fn failure_reason(&self) -> &'static str {
        "OOM detected in kernel journal"
    }

    fn success_message(&self) -> &'static str {
        "OOM watcher healthy"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        let was_enabled = self.cfg.enabled;

        self.cfg = cfg.oom.clone();
        self.interval = cfg.global.tick;

        if was_enabled != self.cfg.enabled {
            info!(
                plugin = self.id(),
                enabled = self.cfg.enabled,
                "OOM plugin enabled state changed: enabled={}",
                self.cfg.enabled
            );
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
                "event-immediate reboot configured, {} pattern(s), debounce {:?}",
                self.cfg.patterns.len(),
                self.cfg.debounce
            ),
        )
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!(
                    "journalctl available, {} pattern(s), debounce {:?}",
                    self.cfg.patterns.len(),
                    self.cfg.debounce
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

        let mut event_count = 0_u32;
        let mut first_line: Option<String> = None;

        while let Ok(event) = self.rx.try_recv() {
            event_count += 1;

            if first_line.is_none() {
                first_line = Some(event.line);
            }
        }

        if event_count > 0 {
            let details = first_line
                .as_deref()
                .map(|line| format!("matched kernel journal line: {}", line));

            warn!(
                plugin = self.id(),
                event_count,
                details = ?details,
"OOM event detected; immediate reboot remediation requested: event_count={} details={:?}",
                event_count,
                details
            );

            TickOutcome::Failure {
                plugin: self.id(),
                failures: 1,
                limit: 1,
                error: None,
                action: Some(ActionPlan::from_action(Action::Reboot)),
                reason: self.failure_reason(),
                details,
            }
        } else {
            TickOutcome::Idle
        }
    }
}
