use anyhow::Result;

pub fn cmd_version() -> Result<()> {
    println!("🛡️  Watchguard");
    println!();
    println!("Version    : {}", env!("CARGO_PKG_VERSION"));
    println!(
        "Git        : {}",
        option_env!("WATCHGUARD_GIT_HASH").unwrap_or("unknown")
    );
    println!(
        "Build Unix : {}",
        option_env!("WATCHGUARD_BUILD_UNIX").unwrap_or("unknown")
    );
    println!(
        "Rust       : {}",
        option_env!("WATCHGUARD_RUSTC_VERSION").unwrap_or("unknown")
    );
    println!("Config     : /etc/watchguard/config.toml");
    Ok(())
}
