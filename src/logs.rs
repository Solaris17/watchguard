use anyhow::{anyhow, Context, Result};
use std::process::Command;

pub fn cmd_logs(
    unit: &str,
    since: Option<&str>,
    lines: Option<u32>,
    boot: bool,
    follow: bool,
) -> Result<()> {
    let mut cmd = Command::new("/usr/bin/journalctl");
    cmd.arg("-u").arg(unit);

    if boot {
        cmd.arg("-b");
    }

    if let Some(since) = since {
        cmd.arg("--since").arg(since);
    }

    if let Some(lines) = lines {
        cmd.arg("-n").arg(lines.to_string());
    }

    if follow {
        cmd.arg("-f");
    }

    let status = cmd
        .status()
        .with_context(|| "running journalctl for watchguard logs")?;

    if !status.success() {
        return Err(anyhow!("journalctl exited with status {}", status));
    }

    Ok(())
}
