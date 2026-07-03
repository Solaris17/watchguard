
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const STATE_DIR: &str = "/var/lib/watchguard";
pub const STATE_FILE: &str = "/var/lib/watchguard/state.json";
const STATE_VERSION: u32 = 1;
const MAX_EVENTS: usize = 200;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchguardState {
    pub version: u32,
    pub created_unix: u64,
    pub updated_unix: u64,
    pub stats: StateStats,
    pub events: Vec<StateEvent>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct StateStats {
    pub total_events: u64,
    pub remediation_started: u64,
    pub remediation_succeeded: u64,
    pub remediation_failed: u64,
    pub remediation_suppressed: u64,
    pub reboot_requests: u64,
    pub oom_events: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateEvent {
    pub unix_ts: u64,
    pub plugin: String,
    pub action: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub details: Option<String>,
    pub command: Vec<String>,
    pub service: Option<String>,
    pub failures: Option<u32>,
    pub limit: Option<u32>,
}

impl WatchguardState {
    fn new() -> Self {
        let now = now_unix();

        Self {
            version: STATE_VERSION,
            created_unix: now,
            updated_unix: now,
            stats: StateStats::default(),
            events: Vec::new(),
        }
    }

    fn push_event(&mut self, event: StateEvent) {
        self.updated_unix = event.unix_ts;
        self.stats.total_events += 1;

        match event.status.as_str() {
            "started" => self.stats.remediation_started += 1,
            "succeeded" | "accepted" => self.stats.remediation_succeeded += 1,
            "failed" | "invalid" => self.stats.remediation_failed += 1,
            "suppressed" | "skipped" => self.stats.remediation_suppressed += 1,
            _ => {}
        }

        if event.action == "reboot" && event.status == "started" {
            self.stats.reboot_requests += 1;
        }

        if event.plugin == "oom" && event.status == "started" {
            self.stats.oom_events += 1;
        }

        self.events.push(event);

        if self.events.len() > MAX_EVENTS {
            let remove = self.events.len() - MAX_EVENTS;
            self.events.drain(0..remove);
        }
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn state_path() -> PathBuf {
    PathBuf::from(STATE_FILE)
}

pub fn ensure_state_dir() -> Result<()> {
    fs::create_dir_all(STATE_DIR).with_context(|| format!("creating {}", STATE_DIR))
}

pub fn load_state() -> Result<WatchguardState> {
    let path = state_path();

    if !path.exists() {
        return Ok(WatchguardState::new());
    }

    let input = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;

    let mut state: WatchguardState =
        serde_json::from_str(&input).with_context(|| format!("parsing {}", path.display()))?;

    if state.version == 0 {
        state.version = STATE_VERSION;
    }

    Ok(state)
}

pub fn write_state(state: &WatchguardState) -> Result<()> {
    ensure_state_dir()?;

    let path = state_path();
    let tmp = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(state).context("serializing watchguard state")?;

    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .with_context(|| format!("opening {}", tmp.display()))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("writing {}", tmp.display()))?;

        file.write_all(b"\n")
            .with_context(|| format!("writing {}", tmp.display()))?;

        file.sync_all()
            .with_context(|| format!("syncing {}", tmp.display()))?;
    }

    fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;

    sync_parent(&path);

    Ok(())
}

fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn record_event(
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
) -> Result<()> {
    let mut state = load_state().unwrap_or_else(|_| WatchguardState::new());

    state.push_event(StateEvent {
        unix_ts: now_unix(),
        plugin: plugin.to_string(),
        action: action.to_string(),
        status: status.to_string(),
        reason: reason.to_string(),
        message: message.into(),
        details: details.map(str::to_string),
        command: command.to_vec(),
        service: service.map(str::to_string),
        failures,
        limit,
    });

    write_state(&state)
}

pub fn cmd_state() -> Result<()> {
    let state = load_state()?;

    println!("🛡️  Watchguard state");
    println!();
    println!("📄 State DB: {}", STATE_FILE);
    println!("Version       : {}", state.version);
    println!("Created Unix  : {}", state.created_unix);
    println!("Updated Unix  : {}", state.updated_unix);
    println!();
    println!("Stats");
    println!("  Total events          : {}", state.stats.total_events);
    println!("  Remediation started   : {}", state.stats.remediation_started);
    println!("  Remediation succeeded : {}", state.stats.remediation_succeeded);
    println!("  Remediation failed    : {}", state.stats.remediation_failed);
    println!("  Remediation suppressed: {}", state.stats.remediation_suppressed);
    println!("  Reboot requests       : {}", state.stats.reboot_requests);
    println!("  OOM events            : {}", state.stats.oom_events);
    println!();

    if let Some(event) = state.events.last() {
        println!("Last event");
        print_event(event);
    } else {
        println!("No events recorded yet.");
    }

    Ok(())
}

pub fn cmd_history(lines: usize) -> Result<()> {
    let state = load_state()?;
    let lines = lines.max(1);

    println!("🛡️  Watchguard remediation history");
    println!("📄 State DB: {}", STATE_FILE);
    println!();

    if state.events.is_empty() {
        println!("No events recorded yet.");
        return Ok(());
    }

    let start = state.events.len().saturating_sub(lines);

    for event in &state.events[start..] {
        print_event(event);
        println!();
    }

    Ok(())
}

fn print_event(event: &StateEvent) {
    println!(
        "{}  plugin={} action={} status={}",
        event.unix_ts, event.plugin, event.action, event.status
    );
    println!("  reason : {}", event.reason);
    println!("  message: {}", event.message);

    if let Some(details) = &event.details {
        println!("  details: {}", details);
    }

    if let Some(service) = &event.service {
        println!("  service: {}", service);
    }

    if !event.command.is_empty() {
        println!("  command: {:?}", event.command);
    }

    if let Some(failures) = event.failures {
        if let Some(limit) = event.limit {
            println!("  failures: {}/{}", failures, limit);
        } else {
            println!("  failures: {}", failures);
        }
    }
}
