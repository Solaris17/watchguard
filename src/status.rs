use anyhow::{Context, Result};
use tokio::runtime::Runtime;

use crate::{config, plugin::format_status_line, registry, state};

pub fn cmd_status(config_path: &str) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;
    let mut plugins = registry::build_plugins(&cfg);

    println!("🛡️  Watchguard status");
    println!();

    for plugin in plugins.iter_mut() {
        let status = plugin.status(&rt);
        println!("{}", format_status_line(&status));
    }

    println!();

    if let Ok(state) = state::load_state() {
        if let Some(event) = state.events.last() {
            println!("🧾 Last recorded event");
            println!(
                "   {} plugin={} action={} status={}",
                event.unix_ts, event.plugin, event.action, event.status
            );
            println!("   reason: {}", event.reason);
        }
        println!("📄 State DB: {}", state::STATE_FILE);
    }

    println!();
    println!("📄 Config: {}", config_path);

    Ok(())
}
