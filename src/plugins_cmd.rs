use anyhow::{Context, Result};
use tokio::runtime::Runtime;

use crate::{config, registry};

pub fn cmd_plugins(config_path: &str) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;
    let mut plugins = registry::build_plugins(&cfg);

    println!("🛡️  Watchguard plugins");
    println!();

    for plugin in plugins.iter_mut() {
        let status = plugin.status(&rt);

        println!("{} {}", status.health.icon(), plugin.id());
        println!("   Name        : {}", plugin.name());
        println!("   Description : {}", plugin.description());
        println!("   Enabled     : {}", plugin.enabled());
        println!("   Interval    : {:?}", plugin.interval());
        println!("   Fail limit  : {}", plugin.fail_limit());
        println!("   Action      : {:?}", plugin.failure_action());
        println!("   Status      : {}", status.message);
        println!();
    }

    println!("📄 Config: {}", config_path);

    Ok(())
}
