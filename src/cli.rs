use clap::{Parser, Subcommand, ValueEnum};

use crate::config::DEFAULT_CONFIG_PATH;

#[derive(Parser)]
#[command(name = "watchguard")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "🛡️ Watchguard host health monitor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Subcommand)]
pub enum CliCommand {
    /// Run Watchguard as a daemon.
    Daemon {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Print enabled plugins and current status.
    Status {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Run environment, config, and live probe diagnostics.
    Doctor {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Run each enabled plugin check once.
    Test {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,

        /// Test all configured plugins, even if disabled.
        #[arg(long)]
        all: bool,
    },

    /// List registered plugins, resolved service names, and remediation policy.
    Plugins {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Show persistent Watchguard state from /var/lib/watchguard/state.json.
    State,

    /// Show persistent remediation history.
    History {
        /// Number of history entries to show.
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: usize,
    },

    /// Follow Watchguard logs using journalctl.
    Logs {
        /// systemd unit to read logs for.
        #[arg(long, default_value = "watchguard.service")]
        unit: String,

        /// Show logs since this time, e.g. "1 hour ago", "today", "2026-07-03 12:00".
        #[arg(long)]
        since: Option<String>,

        /// Number of lines to show before following.
        #[arg(short = 'n', long)]
        lines: Option<u32>,

        /// Show logs from the current boot.
        #[arg(short = 'b', long)]
        boot: bool,

        /// Do not follow logs.
        #[arg(long)]
        no_follow: bool,
    },

    /// Enable a plugin or sub-test.
    Enable {
        plugin: Plugin,

        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Disable a plugin or sub-test.
    Disable {
        plugin: Plugin,

        /// Remove the plugin config table instead of setting enabled=false.
        #[arg(long)]
        remove: bool,

        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Manage Watchguard config.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Print Watchguard version and build metadata.
    Version,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Create a default config if one does not exist.
    Init {
        #[arg(long)]
        force: bool,

        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Print the active config file.
    Show {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Validate the config file.
    Validate {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },

    /// Open the config in $EDITOR, or vi if EDITOR is unset.
    Edit {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Plugin {
    /// Enable or disable the whole SSH plugin.
    Ssh,

    /// Enable or disable only the sshd service-state check.
    #[value(name = "ssh-service")]
    SshService,

    /// Enable or disable only SSH target reachability checks.
    #[value(name = "ssh-targets")]
    SshTargets,

    /// Enable or disable outbound TCP checks.
    Network,

    /// Enable or disable DNS checks.
    Dns,

    /// Enable or disable OOM journal monitoring.
    Oom,
}
