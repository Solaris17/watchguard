use anyhow::{Context, Result};
use std::{fs, path::Path, process::Command};
use tokio::runtime::Runtime;

use crate::{config, plugins, util};

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

    if cfg.ssh.enabled && cfg.ssh.service_check_enabled {
        match rt.block_on(plugins::ssh::systemd_unit_is_active(&cfg.ssh.service)) {
            Ok(true) => println!("✅ SSH service active: {}", cfg.ssh.service),
            Ok(false) => {
                println!("⚠️  SSH service is not active: {}", cfg.ssh.service);
                warnings += 1;
            }
            Err(e) => {
                println!("⚠️  SSH service check error for {}: {}", cfg.ssh.service, e);
                warnings += 1;
            }
        }
    } else {
        println!("ℹ️  SSH service check disabled");
    }

    if cfg.ssh.enabled && cfg.ssh.target_check_enabled {
        if plugins::ssh::targets_ok(&cfg.ssh) {
            println!("✅ SSH target probe succeeded");
        } else {
            println!("⚠️  SSH target probe failed: {:?}", cfg.ssh.targets);
            warnings += 1;
        }
    } else {
        println!("ℹ️  SSH target check disabled");
    }

    if cfg.network.enabled {
        if plugins::network::check(&cfg.network) {
            println!("✅ Network target probe succeeded");
        } else {
            println!("⚠️  Network target probe failed: {:?}", cfg.network.targets);
            warnings += 1;
        }
    } else {
        println!("ℹ️  Network plugin disabled");
    }

    if cfg.dns.enabled {
        if plugins::dns::check(&cfg.dns) {
            println!(
                "✅ DNS probe succeeded: {} via {}",
                cfg.dns.name, cfg.dns.server
            );
        } else {
            println!(
                "⚠️  DNS probe failed: {} via {}",
                cfg.dns.name, cfg.dns.server
            );
            warnings += 1;
        }
    } else {
        println!("ℹ️  DNS plugin disabled");
    }

    if cfg.oom.enabled {
        if plugins::oom::journalctl_exists() {
            println!("✅ OOM watcher prerequisites look good");
        } else {
            println!("❌ OOM plugin enabled but journalctl is missing");
            failures += 1;
        }
    } else {
        println!("ℹ️  OOM plugin disabled");
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
    } else if warnings > 0 {
        println!("Overall Health: ⚠️  WARNING ({} warning(s))", warnings);
    } else {
        println!("Overall Health: ✅ OK");
    }

    // Doctor is primarily diagnostic. Warnings do not fail the command.
    // Hard failures do return non-zero.
    if failures > 0 {
        let status = Command::new("false").status();
        let _ = status;
        anyhow::bail!("doctor found {} failure(s)", failures);
    }

    Ok(())
}
