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

    /// Run environment/config checks useful for troubleshooting.
    Doctor {
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config: String,
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

    /// Print Watchguard version.
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
