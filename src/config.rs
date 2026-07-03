use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use toml_edit::{value, DocumentMut};

use crate::{cli::Plugin, util};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/watchguard/config.toml";

pub const DEFAULT_CONFIG: &str = r#"# Watchguard configuration
#
# Durations use human-readable syntax:
#   500ms, 2s, 30s, 1m, 5m, 30m, 1h
#
# Safer RPM defaults:
#   - all plugin sections are present
#   - plugins start disabled
#   - enable with: watchguard enable ssh|network|dns|oom
#
# For reboot-capable plugins, validate targets first before setting failure_action = "reboot".

[global]
log_level = "info"

boot_grace_period = "5m"
reboot_cooldown = "30m"

tick = "500ms"

[commands]
reboot = ["/usr/bin/systemctl", "reboot", "--force", "--force"]
restart_ssh = ["/usr/bin/systemctl", "restart", "sshd.service"]

[oom]
enabled = false

patterns = [
  "out of memory: kill process",
  "invoked oom-killer",
  "oom-killer",
  "memory cgroup out of memory"
]

[ssh]
enabled = false

# Service state check via systemd D-Bus.
service_check_enabled = true
service = "sshd.service"
service_check_interval = "2s"
service_fail_limit = 3
service_failure_action = "restart"

# SSH reachability checks.
# This is TCP reachability to host:port, not credentialed SSH login.
target_check_enabled = true
ssh_check_interval = "5s"
ssh_timeout = "1500ms"
ssh_fail_limit = 3
require_all = false

targets = [
  "127.0.0.1:22"
]

ssh_failure_action = "restart"

[network]
enabled = false

check_interval = "5s"
timeout = "1500ms"
fail_limit = 6
require_all = false

targets = [
  "1.1.1.1:443",
  "8.8.8.8:443"
]

# Keep as "none" until admins confirm the targets are reliable.
failure_action = "none"

[dns]
enabled = false

check_interval = "30s"
fail_limit = 6

server = "1.1.1.1:53"
name = "example.com"

# Keep as "none" until admins confirm DNS behavior is reliable.
failure_action = "none"
"#;

const DEFAULT_OOM_SECTION: &str = r#"
[oom]
enabled = false

patterns = [
  "out of memory: kill process",
  "invoked oom-killer",
  "oom-killer",
  "memory cgroup out of memory"
]
"#;

const DEFAULT_SSH_SECTION: &str = r#"
[ssh]
enabled = false

service_check_enabled = true
service = "sshd.service"
service_check_interval = "2s"
service_fail_limit = 3
service_failure_action = "restart"

target_check_enabled = true
ssh_check_interval = "5s"
ssh_timeout = "1500ms"
ssh_fail_limit = 3
require_all = false

targets = [
  "127.0.0.1:22"
]

ssh_failure_action = "restart"
"#;

const DEFAULT_NETWORK_SECTION: &str = r#"
[network]
enabled = false

check_interval = "5s"
timeout = "1500ms"
fail_limit = 6
require_all = false

targets = [
  "1.1.1.1:443",
  "8.8.8.8:443"
]

failure_action = "none"
"#;

const DEFAULT_DNS_SECTION: &str = r#"
[dns]
enabled = false

check_interval = "30s"
fail_limit = 6

server = "1.1.1.1:53"
name = "example.com"

failure_action = "none"
"#;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub global: GlobalConfig,
    pub commands: CommandsConfig,
    pub oom: OomConfig,
    pub ssh: SshConfig,
    pub network: NetworkConfig,
    pub dns: DnsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            global: GlobalConfig::default(),
            commands: CommandsConfig::default(),
            oom: OomConfig::default(),
            ssh: SshConfig::default(),
            network: NetworkConfig::default(),
            dns: DnsConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct GlobalConfig {
    pub log_level: String,

    #[serde(with = "humantime_serde")]
    pub boot_grace_period: Duration,

    #[serde(with = "humantime_serde")]
    pub reboot_cooldown: Duration,

    #[serde(with = "humantime_serde")]
    pub tick: Duration,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            boot_grace_period: Duration::from_secs(5 * 60),
            reboot_cooldown: Duration::from_secs(30 * 60),
            tick: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CommandsConfig {
    pub reboot: Vec<String>,
    pub restart_ssh: Vec<String>,
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            reboot: vec![
                "/usr/bin/systemctl".to_string(),
                "reboot".to_string(),
                "--force".to_string(),
                "--force".to_string(),
            ],
            restart_ssh: vec![
                "/usr/bin/systemctl".to_string(),
                "restart".to_string(),
                "sshd.service".to_string(),
            ],
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct OomConfig {
    pub enabled: bool,
    pub patterns: Vec<String>,
}

impl Default for OomConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            patterns: vec![
                "out of memory: kill process".to_string(),
                "invoked oom-killer".to_string(),
                "oom-killer".to_string(),
                "memory cgroup out of memory".to_string(),
            ],
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SshConfig {
    pub enabled: bool,

    pub service_check_enabled: bool,
    pub service: String,

    #[serde(with = "humantime_serde")]
    pub service_check_interval: Duration,

    pub service_fail_limit: u32,
    pub service_failure_action: Action,

    pub target_check_enabled: bool,

    #[serde(with = "humantime_serde")]
    pub ssh_check_interval: Duration,

    #[serde(with = "humantime_serde")]
    pub ssh_timeout: Duration,

    pub ssh_fail_limit: u32,
    pub require_all: bool,
    pub targets: Vec<String>,
    pub ssh_failure_action: Action,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_check_enabled: true,
            service: "sshd.service".to_string(),
            service_check_interval: Duration::from_secs(2),
            service_fail_limit: 3,
            service_failure_action: Action::Restart,
            target_check_enabled: true,
            ssh_check_interval: Duration::from_secs(5),
            ssh_timeout: Duration::from_millis(1500),
            ssh_fail_limit: 3,
            require_all: false,
            targets: vec!["127.0.0.1:22".to_string()],
            ssh_failure_action: Action::Restart,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct NetworkConfig {
    pub enabled: bool,

    #[serde(with = "humantime_serde")]
    pub check_interval: Duration,

    #[serde(with = "humantime_serde")]
    pub timeout: Duration,

    pub fail_limit: u32,
    pub require_all: bool,
    pub targets: Vec<String>,
    pub failure_action: Action,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval: Duration::from_secs(5),
            timeout: Duration::from_millis(1500),
            fail_limit: 6,
            require_all: false,
            targets: vec!["1.1.1.1:443".to_string(), "8.8.8.8:443".to_string()],
            failure_action: Action::None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct DnsConfig {
    pub enabled: bool,

    #[serde(with = "humantime_serde")]
    pub check_interval: Duration,

    pub fail_limit: u32,
    pub server: String,
    pub name: String,
    pub failure_action: Action,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval: Duration::from_secs(30),
            fail_limit: 6,
            server: "1.1.1.1:53".to_string(),
            name: "example.com".to_string(),
            failure_action: Action::None,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Restart,
    Reboot,
    None,
}

impl Default for Action {
    fn default() -> Self {
        Action::None
    }
}

pub fn load_config(path: &str) -> Result<AppConfig> {
    let s = fs::read_to_string(path).with_context(|| format!("reading config {}", path))?;
    let cfg: AppConfig = toml::from_str(&s).with_context(|| format!("parsing TOML {}", path))?;
    validate_config(&cfg)?;
    Ok(cfg)
}

pub fn validate_config(cfg: &AppConfig) -> Result<()> {
    if cfg.global.tick < Duration::from_millis(100) {
        return Err(anyhow!("global.tick must be >= 100ms"));
    }

    if cfg.global.boot_grace_period > Duration::from_secs(24 * 60 * 60) {
        return Err(anyhow!("global.boot_grace_period is unreasonably large"));
    }

    if cfg.global.reboot_cooldown > Duration::from_secs(7 * 24 * 60 * 60) {
        return Err(anyhow!("global.reboot_cooldown is unreasonably large"));
    }

    if cfg.commands.reboot.is_empty() {
        return Err(anyhow!("commands.reboot must not be empty"));
    }

    if cfg.commands.restart_ssh.is_empty() {
        return Err(anyhow!("commands.restart_ssh must not be empty"));
    }

    if cfg.oom.enabled && cfg.oom.patterns.is_empty() {
        return Err(anyhow!("oom.patterns must not be empty when oom.enabled=true"));
    }

    if cfg.ssh.enabled {
        if cfg.ssh.service_check_enabled {
            if cfg.ssh.service.trim().is_empty() {
                return Err(anyhow!("ssh.service must not be empty"));
            }

            if cfg.ssh.service_fail_limit == 0 {
                return Err(anyhow!("ssh.service_fail_limit must be >= 1"));
            }
        }

        if cfg.ssh.target_check_enabled {
            if cfg.ssh.targets.is_empty() {
                return Err(anyhow!("ssh.targets must not be empty"));
            }

            if cfg.ssh.ssh_fail_limit == 0 {
                return Err(anyhow!("ssh.ssh_fail_limit must be >= 1"));
            }

            util::validate_targets("ssh.targets", &cfg.ssh.targets)?;
        }
    }

    if cfg.network.enabled {
        if cfg.network.targets.is_empty() {
            return Err(anyhow!("network.targets must not be empty"));
        }

        if cfg.network.fail_limit == 0 {
            return Err(anyhow!("network.fail_limit must be >= 1"));
        }

        util::validate_targets("network.targets", &cfg.network.targets)?;
    }

    if cfg.dns.enabled {
        if cfg.dns.fail_limit == 0 {
            return Err(anyhow!("dns.fail_limit must be >= 1"));
        }

        util::validate_socket_addr("dns.server", &cfg.dns.server)?;

        if cfg.dns.name.trim().is_empty() {
            return Err(anyhow!("dns.name must not be empty"));
        }
    }

    Ok(())
}

pub fn cmd_config_init(config_path: &str, force: bool) -> Result<()> {
    let path = PathBuf::from(config_path);

    if path.exists() && !force {
        println!("⚠️  Config already exists: {}", config_path);
        println!("Use --force to overwrite it.");
        return Ok(());
    }

    write_string_to_file(&path, DEFAULT_CONFIG)?;
    println!("✅ Wrote default config: {}", config_path);
    Ok(())
}

pub fn cmd_config_show(config_path: &str) -> Result<()> {
    let s = fs::read_to_string(config_path)
        .with_context(|| format!("reading config {}", config_path))?;

    print!("{}", s);
    Ok(())
}

pub fn cmd_config_validate(config_path: &str) -> Result<()> {
    println!("🛡️  Validating Watchguard configuration...");
    println!();

    match load_config(config_path) {
        Ok(_) => {
            println!("✅ TOML syntax");
            println!("✅ Global settings");
            println!("✅ Commands");
            println!("✅ OOM plugin");
            println!("✅ SSH plugin");
            println!("✅ Network plugin");
            println!("✅ DNS plugin");
            println!();
            println!("Configuration is valid.");
            Ok(())
        }
        Err(e) => {
            println!("❌ Configuration error:");
            println!("{:#}", e);
            Err(e)
        }
    }
}

pub fn cmd_config_edit(config_path: &str) -> Result<()> {
    ensure_config_exists(config_path)?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(editor)
        .arg(config_path)
        .status()
        .context("opening editor")?;

    if !status.success() {
        return Err(anyhow!("editor exited with status {}", status));
    }

    cmd_config_validate(config_path)
}

pub fn cmd_enable_disable(
    config_path: &str,
    plugin: Plugin,
    enabled: bool,
    remove: bool,
) -> Result<()> {
    ensure_config_exists(config_path)?;

    let input = fs::read_to_string(config_path)
        .with_context(|| format!("reading config {}", config_path))?;

    let mut doc = input
        .parse::<DocumentMut>()
        .with_context(|| format!("parsing config {}", config_path))?;

    if enabled {
        ensure_plugin_section(&mut doc, &plugin)?;
        set_plugin_enabled(&mut doc, &plugin, true)?;
        write_string_to_file(Path::new(config_path), &doc.to_string())?;

        println!("✅ Enabled {}", plugin_name(&plugin));
        println!("📄 Updated {}", config_path);
        return Ok(());
    }

    if remove {
        remove_plugin_section(&mut doc, &plugin);
        write_string_to_file(Path::new(config_path), &doc.to_string())?;

        println!("🗑️  Removed config for {}", plugin_name(&plugin));
        println!("📄 Updated {}", config_path);
        return Ok(());
    }

    ensure_plugin_section(&mut doc, &plugin)?;
    set_plugin_enabled(&mut doc, &plugin, false)?;
    write_string_to_file(Path::new(config_path), &doc.to_string())?;

    println!("❌ Disabled {}", plugin_name(&plugin));
    println!("📄 Updated {}", config_path);
    println!("Tip: use --remove to remove the config table entirely.");

    Ok(())
}

fn ensure_config_exists(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);

    if !path.exists() {
        write_string_to_file(path, DEFAULT_CONFIG)?;
    }

    Ok(())
}

fn write_string_to_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }

    fs::write(path, content)
        .with_context(|| format!("writing {}", path.display()))?;

    Ok(())
}

fn ensure_plugin_section(doc: &mut DocumentMut, plugin: &Plugin) -> Result<()> {
    match plugin {
        Plugin::Ssh | Plugin::SshService | Plugin::SshTargets => {
            if doc.get("ssh").is_none() {
                let fragment = DEFAULT_SSH_SECTION.parse::<DocumentMut>()?;
                doc["ssh"] = fragment["ssh"].clone();
            }
        }
        Plugin::Network => {
            if doc.get("network").is_none() {
                let fragment = DEFAULT_NETWORK_SECTION.parse::<DocumentMut>()?;
                doc["network"] = fragment["network"].clone();
            }
        }
        Plugin::Dns => {
            if doc.get("dns").is_none() {
                let fragment = DEFAULT_DNS_SECTION.parse::<DocumentMut>()?;
                doc["dns"] = fragment["dns"].clone();
            }
        }
        Plugin::Oom => {
            if doc.get("oom").is_none() {
                let fragment = DEFAULT_OOM_SECTION.parse::<DocumentMut>()?;
                doc["oom"] = fragment["oom"].clone();
            }
        }
    }

    Ok(())
}

fn set_plugin_enabled(doc: &mut DocumentMut, plugin: &Plugin, enabled: bool) -> Result<()> {
    match plugin {
        Plugin::Ssh => {
            doc["ssh"]["enabled"] = value(enabled);
        }
        Plugin::SshService => {
            doc["ssh"]["enabled"] = value(true);
            doc["ssh"]["service_check_enabled"] = value(enabled);
        }
        Plugin::SshTargets => {
            doc["ssh"]["enabled"] = value(true);
            doc["ssh"]["target_check_enabled"] = value(enabled);
        }
        Plugin::Network => {
            doc["network"]["enabled"] = value(enabled);
        }
        Plugin::Dns => {
            doc["dns"]["enabled"] = value(enabled);
        }
        Plugin::Oom => {
            doc["oom"]["enabled"] = value(enabled);
        }
    }

    Ok(())
}

fn remove_plugin_section(doc: &mut DocumentMut, plugin: &Plugin) {
    match plugin {
        Plugin::Ssh | Plugin::SshService | Plugin::SshTargets => {
            doc.as_table_mut().remove("ssh");
        }
        Plugin::Network => {
            doc.as_table_mut().remove("network");
        }
        Plugin::Dns => {
            doc.as_table_mut().remove("dns");
        }
        Plugin::Oom => {
            doc.as_table_mut().remove("oom");
        }
    }
}

fn plugin_name(plugin: &Plugin) -> &'static str {
    match plugin {
        Plugin::Ssh => "ssh",
        Plugin::SshService => "ssh-service",
        Plugin::SshTargets => "ssh-targets",
        Plugin::Network => "network",
        Plugin::Dns => "dns",
        Plugin::Oom => "oom",
    }
}
