use anyhow::{Context, Result};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

use crate::{config, plugin::format_status_line, registry, util};

pub fn cmd_doctor(config_path: &str) -> Result<()> {
    println!("🛡️  Watchguard Doctor");
    println!();

    let mut warnings = 0_u32;
    let mut failures = 0_u32;

    let cfg = match config::load_config(config_path) {
        Ok(cfg) => {
            println!("✅ Config syntax");
            cfg
        }
        Err(e) => {
            println!("❌ Config syntax: {:#}", e);
            return Err(e);
        }
    };

    if Path::new(config_path).exists() {
        println!("✅ Config file exists: {}", config_path);
    } else {
        println!("❌ Config file missing: {}", config_path);
        failures += 1;
    }

    if let Ok(meta) = fs::metadata(config_path) {
        if meta.permissions().readonly() {
            println!("⚠️  Config file is read-only");
            warnings += 1;
        } else {
            println!("✅ Config file permissions allow writes");
        }
    }

    if util::command_exists("/usr/bin/systemctl") {
        println!("✅ systemctl found");
    } else {
        println!("❌ /usr/bin/systemctl not found");
        failures += 1;
    }

    if util::command_exists("/usr/bin/journalctl") {
        println!("✅ journalctl found");
    } else if cfg.oom.enabled {
        println!("❌ OOM plugin enabled but /usr/bin/journalctl not found");
        failures += 1;
    } else {
        println!("⚠️  journalctl not found, OOM plugin currently disabled");
        warnings += 1;
    }

    if Path::new("/usr/lib/systemd/system/watchguard.service").exists()
        || Path::new("/etc/systemd/system/watchguard.service").exists()
    {
        println!("✅ systemd service file found");
    } else {
        println!("⚠️  systemd service file not found in common paths");
        warnings += 1;
    }

    if !cfg.commands.reboot.is_empty() {
        println!("✅ Reboot command configured: {:?}", cfg.commands.reboot);
    } else {
        println!("❌ Reboot command missing");
        failures += 1;
    }

    if !cfg.commands.restart_ssh.is_empty() {
        println!(
            "✅ SSH restart command configured: {:?}",
            cfg.commands.restart_ssh
        );
    } else {
        println!("❌ SSH restart command missing");
        failures += 1;
    }

    let rt = Runtime::new().context("creating Tokio runtime")?;
    let mut plugins = registry::build_plugins(&cfg);

    for plugin in plugins.iter_mut() {
        for status in plugin.doctor(&rt) {
            if status.health.is_failure() {
                failures += 1;
            }

            if status.health.is_warning() {
                warnings += 1;
            }

            println!("{}", format_status_line(&status));
        }
    }

    println!();
    println!(
        "ℹ️  Boot grace configured: {:?}",
        cfg.global.boot_grace_period
    );
    println!(
        "ℹ️  Reboot cooldown configured: {:?}",
        cfg.global.reboot_cooldown
    );
    println!();

    if failures > 0 {
        println!(
            "Overall Health: ❌ FAILED ({} failure(s), {} warning(s))",
            failures, warnings
        );
        anyhow::bail!("doctor found {} failure(s)", failures);
    } else if warnings > 0 {
        println!("Overall Health: ⚠️  WARNING ({} warning(s))", warnings);
    } else {
        println!("Overall Health: ✅ OK");
    }

    Ok(())
}
