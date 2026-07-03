use anyhow::{Context, Result};
use tokio::runtime::Runtime;

use crate::{config, plugins};

pub fn cmd_status(config_path: &str) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;

    println!("🛡️  Watchguard status");
    println!();

    if cfg.oom.enabled {
        if plugins::oom::journalctl_exists() {
            println!(
                "✅ oom          enabled   journalctl watcher configured, {} pattern(s)",
                cfg.oom.patterns.len()
            );
        } else {
            println!("⚠️  oom          enabled   /usr/bin/journalctl not found");
        }
    } else {
        println!("❌ oom          disabled");
    }

    if cfg.ssh.enabled {
        if cfg.ssh.service_check_enabled {
            match rt.block_on(plugins::ssh::systemd_unit_is_active(&cfg.ssh.service)) {
                Ok(true) => println!("✅ ssh-service  enabled   {} is active", cfg.ssh.service),
                Ok(false) => println!("⚠️  ssh-service  enabled   {} is not active", cfg.ssh.service),
                Err(e) => println!("⚠️  ssh-service  enabled   status error: {}", e),
            }
        } else {
            println!("❌ ssh-service  disabled");
        }

        if cfg.ssh.target_check_enabled {
            let ok = plugins::ssh::targets_ok(&cfg.ssh);

            if ok {
                println!(
                    "✅ ssh-targets  enabled   {} target(s) configured",
                    cfg.ssh.targets.len()
                );
            } else {
                println!(
                    "⚠️  ssh-targets  enabled   target probe failed, {} target(s) configured",
                    cfg.ssh.targets.len()
                );
            }
        } else {
            println!("❌ ssh-targets  disabled");
        }
    } else {
        println!("❌ ssh          disabled");
    }

    if cfg.network.enabled {
        let ok = plugins::network::check(&cfg.network);

        if ok {
            println!(
                "✅ network      enabled   {} target(s) configured",
                cfg.network.targets.len()
            );
        } else {
            println!(
                "⚠️  network      enabled   target probe failed, {} target(s) configured",
                cfg.network.targets.len()
            );
        }
    } else {
        println!("❌ network      disabled");
    }

    if cfg.dns.enabled {
        let ok = plugins::dns::check(&cfg.dns);

        if ok {
            println!("✅ dns          enabled   {} via {}", cfg.dns.name, cfg.dns.server);
        } else {
            println!(
                "⚠️  dns          enabled   DNS probe failed: {} via {}",
                cfg.dns.name, cfg.dns.server
            );
        }
    } else {
        println!("❌ dns          disabled");
    }

    println!();
    println!("📄 Config: {}", config_path);

    Ok(())
}
