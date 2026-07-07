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

// Set the line capture & time capture max
const OOM_CONTEXT_CAPTURE_SECS: u64 = 5;
const OOM_CONTEXT_MAX_LINES: usize = 500;
const OOM_CONTEXT_SUMMARY_MAX_LINES: usize = 80;

#[derive(Debug, Clone)]
pub struct OomEvent {
    pub first_line: String,
    pub context: Vec<String>,
}

#[derive(Debug, Clone)]
struct PendingOom {
    first_line: String,
    context: Vec<String>,
    deadline: Instant,
}

#[derive(Debug, Clone)]
struct ContainerOom {
    runtime: &'static str,
    id: Option<String>,
}

pub fn journalctl_exists() -> bool {
    util::command_exists("/usr/bin/journalctl")
}

fn oom_trigger_patterns(mut patterns: Vec<String>) -> Vec<String> {
    // These built-ins are deliberately added even if the config only contains
    // older strings such as "invoked oom-killer". Some memcg/container OOMs
    // start with the later "oom-kill:" line, which also contains the best
    // classification data: constraint, cpuset, oom_memcg, task_memcg, task, pid.
    let builtins = [
        "invoked oom-killer",
        "oom-kill:",
        "memory cgroup out of memory",
        "memory cgroup stats for",
        "oom_memcg=",
        "task_memcg=",
    ];

    for builtin in builtins {
        if !patterns.iter().any(|p| p == builtin) {
            patterns.push(builtin.to_string());
        }
    }

    patterns
}

fn is_interesting_context_line(lower: &str) -> bool {
    lower.contains("oom")
        || lower.contains("out of memory")
        || lower.contains("memory cgroup")
        || lower.contains("killed process")
        || lower.contains("constraint=")
        || lower.contains("cpuset=")
        || lower.contains("oom_memcg=")
        || lower.contains("task_memcg=")
        || lower.contains("docker-")
        || lower.contains("containerd")
        || lower.contains("kubepods")
        || lower.contains("libpod")
        || lower.contains("podman")
}

fn push_pending_context(pending: &mut PendingOom, line: String) {
    if pending.context.len() < OOM_CONTEXT_MAX_LINES {
        pending.context.push(line);
    }
}

fn emit_pending(
    pending: &mut Option<PendingOom>,
    last_sent: &mut Option<Instant>,
    debounce: Duration,
    oom_tx: &mpsc::Sender<OomEvent>,
) {
    let Some(pending_event) = pending.take() else {
        return;
    };

    let now = Instant::now();

    if last_sent
        .map(|last| now.duration_since(last) < debounce)
        .unwrap_or(false)
    {
        debug!(
            target = "oom",
            first_line = %pending_event.first_line,
            context_lines = pending_event.context.len(),
            debounce = ?debounce,
            "duplicate OOM event suppressed by debounce: debounce={:?} first_line={} context_lines={}",
            debounce,
            pending_event.first_line,
            pending_event.context.len()
        );
        return;
    }

    *last_sent = Some(now);

    warn!(
        target = "oom",
        first_line = %pending_event.first_line,
        context_lines = pending_event.context.len(),
        capture_secs = OOM_CONTEXT_CAPTURE_SECS,
        "OOM kernel event captured: first_line={} context_lines={} capture_secs={}",
        pending_event.first_line,
        pending_event.context.len(),
        OOM_CONTEXT_CAPTURE_SECS
    );

    let _ = oom_tx.send(OomEvent {
        first_line: pending_event.first_line,
        context: pending_event.context,
    });
}

fn extract_after_marker(context: &str, marker: &str) -> Option<String> {
    let start = context.find(marker)? + marker.len();
    let mut id = String::new();

    for ch in context[start..].chars() {
        if ch.is_ascii_hexdigit() {
            id.push(ch);
        } else {
            break;
        }
    }

    if id.len() >= 12 {
        Some(id)
    } else {
        None
    }
}

fn classify_container_oom(context: &str) -> Option<ContainerOom> {
    let lower = context.to_lowercase();

    // Suppress host reboot only when the cgroup path clearly belongs to a
    // container runtime. Do not suppress merely because this is a memcg OOM;
    // systemd services and user slices also use memory cgroups.
    if lower.contains("/system.slice/docker-")
        || lower.contains("cpuset=docker-")
        || lower.contains("oom_memcg=/system.slice/docker-")
        || lower.contains("task_memcg=/system.slice/docker-")
        || (lower.contains("docker-") && lower.contains(".scope"))
    {
        return Some(ContainerOom {
            runtime: "docker",
            id: extract_after_marker(&lower, "docker-"),
        });
    }

    if lower.contains("cri-containerd-")
        || lower.contains("/containerd/")
        || lower.contains("containerd.service")
    {
        return Some(ContainerOom {
            runtime: "containerd",
            id: extract_after_marker(&lower, "cri-containerd-"),
        });
    }

    if lower.contains("kubepods") {
        return Some(ContainerOom {
            runtime: "kubernetes",
            id: None,
        });
    }

    if lower.contains("/machine.slice/libpod-")
        || lower.contains("libpod-")
        || lower.contains("podman")
    {
        return Some(ContainerOom {
            runtime: "podman",
            id: extract_after_marker(&lower, "libpod-"),
        });
    }

    None
}

fn summarize_context(context: &[String]) -> String {
    let mut selected = Vec::new();

    for line in context {
        let lower = line.to_lowercase();

        if selected.is_empty() || is_interesting_context_line(&lower) {
            selected.push(line.clone());
        }

        if selected.len() >= OOM_CONTEXT_SUMMARY_MAX_LINES {
            selected.push(format!(
                "... context truncated after {} selected line(s); captured {} total line(s)",
                OOM_CONTEXT_SUMMARY_MAX_LINES,
                context.len()
            ));
            break;
        }
    }

    if selected.is_empty() {
        return "-- no OOM context captured --".to_string();
    }

    selected.join("\n")
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

    let patterns: Vec<String> =
        oom_trigger_patterns(patterns.into_iter().map(|p| p.to_lowercase()).collect());

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(|line| line.ok()) {
            debug!(target = "oom", "journalctl stderr: {}", line);
        }
    });

    let (line_tx, line_rx) = mpsc::channel::<String>();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);

        for line in reader.lines().map_while(|line| line.ok()) {
            if line_tx.send(line).is_err() {
                break;
            }
        }

        warn!(target = "oom", "OOM journal watcher stdout ended");
    });

    thread::spawn(move || {
        let mut last_sent: Option<Instant> = None;
        let mut pending: Option<PendingOom> = None;

        loop {
            if pending
                .as_ref()
                .map(|event| Instant::now() >= event.deadline)
                .unwrap_or(false)
            {
                emit_pending(&mut pending, &mut last_sent, debounce, &oom_tx);
                continue;
            }

            let timeout = pending
                .as_ref()
                .map(|event| event.deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|| Duration::from_secs(3600));

            match line_rx.recv_timeout(timeout) {
                Ok(line) => {
                    let lower = line.to_lowercase();
                    let is_trigger = patterns.iter().any(|p| lower.contains(p));

                    if let Some(event) = pending.as_mut() {
                        // Once an OOM starts, capture a short live window of all kernel
                        // lines, not only lines matching OOM keywords. The useful
                        // docker/container evidence can appear after stack traces or task tables.
                        push_pending_context(event, line.clone());
                    }

                    if is_trigger {
                        if pending.is_none() {
                            let deadline = Instant::now()
                                .checked_add(Duration::from_secs(OOM_CONTEXT_CAPTURE_SECS))
                                .unwrap_or_else(Instant::now);

                            warn!(
                                target = "oom",
                                journal_line = %line,
                                capture_secs = OOM_CONTEXT_CAPTURE_SECS,
                                "OOM kernel pattern matched; collecting context before remediation decision: capture_secs={} line={}",
                                OOM_CONTEXT_CAPTURE_SECS,
                                line
                            );

                            pending = Some(PendingOom {
                                first_line: line.clone(),
                                context: vec![line],
                                deadline,
                            });
                        } else {
                            debug!(
                                target = "oom",
                                journal_line = %line,
                                "additional OOM pattern matched while context capture is pending: line={}",
                                line
                            );
                        }
                    }

                    if pending
                        .as_ref()
                        .map(|event| Instant::now() >= event.deadline)
                        .unwrap_or(false)
                    {
                        emit_pending(&mut pending, &mut last_sent, debounce, &oom_tx);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    emit_pending(&mut pending, &mut last_sent, debounce, &oom_tx);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    emit_pending(&mut pending, &mut last_sent, debounce, &oom_tx);
                    warn!(target = "oom", "OOM journal line channel disconnected");
                    break;
                }
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
        "Watches kernel journald messages for OOM events, suppressing host reboot only for proven container OOMs"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    fn remediation_mode(&self) -> &'static str {
        "event-context-classified"
    }

    fn remediation_summary(&self) -> Option<String> {
        Some(format!(
            "collect {}s of OOM context; suppress reboot only for Docker/container runtime OOMs; debounce {:?}",
            OOM_CONTEXT_CAPTURE_SECS,
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
        Ok(journalctl_exists())
    }

    fn status(&mut self, _rt: &Runtime) -> PluginStatus {
        if !self.enabled() {
            return PluginStatus::disabled(self.id(), "disabled");
        }

        if !journalctl_exists() {
            return PluginStatus::failed(self.id(), "/usr/bin/journalctl not found");
        }

        PluginStatus::healthy(
            self.id(),
            format!(
                "event-context-classified reboot configured, {} configured pattern(s), built-in OOM patterns appended, {}s context capture, debounce {:?}",
                self.cfg.patterns.len(),
                OOM_CONTEXT_CAPTURE_SECS,
                self.cfg.debounce
            ),
        )
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!(
                    "journalctl available, {} configured pattern(s), built-in OOM patterns appended, {}s context capture, debounce {:?}",
                    self.cfg.patterns.len(),
                    OOM_CONTEXT_CAPTURE_SECS,
                    self.cfg.debounce
                ),
            ),
            Ok(false) => PluginStatus::failed(
                self.id(),
                "/usr/bin/journalctl unavailable",
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
        let mut first_container_event: Option<(OomEvent, ContainerOom)> = None;
        let mut first_reboot_event: Option<OomEvent> = None;

        while let Ok(event) = self.rx.try_recv() {
            event_count += 1;

            let context_text = event.context.join("\n");

            if let Some(container) = classify_container_oom(&context_text) {
                warn!(
                    plugin = self.id(),
                    runtime = container.runtime,
                    container_id = container.id.as_deref().unwrap_or("unknown"),
                    first_line = %event.first_line,
                    context_lines = event.context.len(),
                    "OOM event classified as container OOM; host reboot suppressed: runtime={} container_id={} first_line={} context_lines={}",
                    container.runtime,
                    container.id.as_deref().unwrap_or("unknown"),
                    event.first_line,
                    event.context.len()
                );

                if first_container_event.is_none() {
                    first_container_event = Some((event, container));
                }
            } else {
                warn!(
                    plugin = self.id(),
                    first_line = %event.first_line,
                    context_lines = event.context.len(),
                    "OOM event was not proven to be container-scoped; reboot remains allowed: first_line={} context_lines={}",
                    event.first_line,
                    event.context.len()
                );

                if first_reboot_event.is_none() {
                    first_reboot_event = Some(event);
                }
            }
        }

        if let Some(event) = first_reboot_event {
            let context_summary = summarize_context(&event.context);
            let details = Some(format!(
                "matched kernel journal line: {}; classification=non_container_or_unknown_oom; reboot allowed; context:\n{}",
                event.first_line, context_summary
            ));

            warn!(
                plugin = self.id(),
                event_count,
                details = ?details,
                "OOM event detected; reboot remediation requested: event_count={} details={:?}",
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
        } else if let Some((event, container)) = first_container_event {
            let context_summary = summarize_context(&event.context);
            let details = Some(format!(
                "matched kernel journal line: {}; classification=container_oom; runtime={}; container_id={}; reboot suppressed; context:\n{}",
                event.first_line,
                container.runtime,
                container.id.as_deref().unwrap_or("unknown"),
                context_summary
            ));

            warn!(
                plugin = self.id(),
                event_count,
                runtime = container.runtime,
                container_id = container.id.as_deref().unwrap_or("unknown"),
                details = ?details,
                "container OOM event detected; host reboot suppressed: event_count={} runtime={} container_id={} details={:?}",
                event_count,
                container.runtime,
                container.id.as_deref().unwrap_or("unknown"),
                details
            );

            TickOutcome::Failure {
                plugin: self.id(),
                failures: 1,
                limit: 1,
                error: None,
                action: None,
                reason: "Container OOM detected in kernel journal",
                details,
            }
        } else {
            TickOutcome::Idle
        }
    }
}
