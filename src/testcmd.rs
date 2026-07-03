use anyhow::{bail, Context, Result};
use tokio::runtime::Runtime;

use crate::{
    config,
    plugin::{format_status_line, PluginStatus},
    registry,
};

pub fn cmd_test(config_path: &str, all: bool) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;
    let mut plugins = registry::build_plugins(&cfg);

    println!("🛡️  Testing Watchguard plugins...");
    println!();

    let mut failures = 0_u32;
    let mut skipped = 0_u32;

    for plugin in plugins.iter_mut() {
        let status = if plugin.enabled() || all {
            plugin.test(&rt)
        } else {
            PluginStatus::skipped(plugin.id(), "skipped disabled plugin")
        };

        if status.health.is_failure() {
            failures += 1;
        }

        if matches!(status.health, crate::plugin::Health::Skipped) {
            skipped += 1;
        }

        println!("{}", format_status_line(&status));
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
