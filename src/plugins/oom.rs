use anyhow::{anyhow, Context, Result};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
};
use tracing::{debug, warn};

use crate::util;

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
        for line in reader.lines().flatten() {
            debug!(target = "oom", "journalctl stderr: {}", line);
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stdout);

        for line in reader.lines().flatten() {
            let lower = line.to_lowercase();

            if patterns.iter().any(|p| lower.contains(p)) {
                warn!(target = "oom", "OOM pattern matched: {}", line);
                let _ = oom_tx.send(());
            }
        }
    });

    Ok(child)
}
