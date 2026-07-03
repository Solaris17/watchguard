use anyhow::{bail, Context, Result};
use tokio::runtime::Runtime;

use crate::{config, plugins};

pub fn cmd_test(config_path: &str, all: bool) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;

    println!("🛡️  Testing Watchguard plugins...");
    println!();

    let mut failures = 0_u32;
    let mut skipped = 0_u32;

    if cfg.oom.enabled || all {
        if plugins::oom::journalctl_exists() {
            println!(
                "✅ oom          journalctl available, {} pattern(s)",
                cfg.oom.patterns.len()
            );
        } else {
            println!("❌ oom          /usr/bin/journalctl not found");
            failures += 1;
        }
    } else {
        println!("⏭️  oom          skipped disabled plugin");
        skipped += 1;
    }

    if cfg.ssh.enabled || all {
        if cfg.ssh.service_check_enabled || all {
            match rt.block_on(plugins::ssh::systemd_unit_is_active(&cfg.ssh.service)) {
                Ok(true) => println!("✅ ssh-service  {} active", cfg.ssh.service),
                Ok(false) => {
                    println!("❌ ssh-service  {} not active", cfg.ssh.service);
                    failures += 1;
                }
                Err(e) => {
                    println!("❌ ssh-service  error: {}", e);
                    failures += 1;
                }
            }
        }

        if cfg.ssh.target_check_enabled || all {
            if plugins::ssh::targets_ok(&cfg.ssh) {
                println!("✅ ssh-targets  target probe succeeded");
            } else {
                println!("❌ ssh-targets  target probe failed: {:?}", cfg.ssh.targets);
                failures += 1;
            }
        }
    } else {
        println!("⏭️  ssh          skipped disabled plugin");
        skipped += 1;
    }

    if cfg.network.enabled || all {
        if plugins::network::check(&cfg.network) {
            println!("✅ network      target probe succeeded");
        } else {
            println!(
                "❌ network      target probe failed: {:?}",
                cfg.network.targets
            );
            failures += 1;
        }
    } else {
        println!("⏭️  network      skipped disabled plugin");
        skipped += 1;
    }

    if cfg.dns.enabled || all {
        if plugins::dns::check(&cfg.dns) {
            println!("✅ dns          {} via {}", cfg.dns.name, cfg.dns.server);
        } else {
            println!(
                "❌ dns          failed: {} via {}",
                cfg.dns.name, cfg.dns.server
            );
            failures += 1;
        }
    } else {
        println!("⏭️  dns          skipped disabled plugin");
        skipped += 1;
    }

    println!();

    if failures > 0 {
        println!(
            "Overall Test: ❌ FAILED ({} failure(s), {} skipped)",
            failures, skipped
        );
        bail!("watchguard test failed");
    }

    println!("Overall Test: ✅ OK ({} skipped)", skipped);
    Ok(())
}
