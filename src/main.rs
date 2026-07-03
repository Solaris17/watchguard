mod actions;
mod cli;
mod config;
mod daemon;
mod doctor;
mod logs;
mod plugins;
mod status;
mod testcmd;
mod util;
mod version;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, CliCommand, ConfigCommand};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        CliCommand::Daemon { config } => daemon::daemon_loop(&config),

        CliCommand::Status { config } => status::cmd_status(&config),

        CliCommand::Doctor { config } => doctor::cmd_doctor(&config),

        CliCommand::Test { config, all } => testcmd::cmd_test(&config, all),

        CliCommand::Logs {
            unit,
            since,
            lines,
            boot,
            no_follow,
        } => logs::cmd_logs(&unit, since.as_deref(), lines, boot, !no_follow),

        CliCommand::Enable { plugin, config } => {
            config::cmd_enable_disable(&config, plugin, true, false)
        }

        CliCommand::Disable {
            plugin,
            remove,
            config,
        } => config::cmd_enable_disable(&config, plugin, false, remove),

        CliCommand::Config { command } => match command {
            ConfigCommand::Init { force, config } => config::cmd_config_init(&config, force),
            ConfigCommand::Show { config } => config::cmd_config_show(&config),
            ConfigCommand::Validate { config } => config::cmd_config_validate(&config),
            ConfigCommand::Edit { config } => config::cmd_config_edit(&config),
        },

        CliCommand::Version => version::cmd_version(),
    }
}
