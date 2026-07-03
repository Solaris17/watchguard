use anyhow::{Context, Result};
use tokio::runtime::Runtime;

use crate::{
    config::{self, Action, EscalationStep},
    plugin::resolved_escalation_steps_for,
    registry,
};

pub fn cmd_plugins(config_path: &str) -> Result<()> {
    let cfg = config::load_config(config_path)?;
    let rt = Runtime::new().context("creating Tokio runtime")?;
    let mut plugins = registry::build_plugins(&cfg);

    println!("🛡️  Watchguard plugins");
    println!();

    for plugin in plugins.iter_mut() {
        let status = plugin.status(&rt);
        let remediation = plugin
            .remediation_summary()
            .unwrap_or_else(|| format_plan(&resolved_escalation_steps_for(plugin.as_ref())));

        println!("{} {}", status.health.icon(), plugin.id());
        println!("   Name        : {}", plugin.name());
        println!("   Description : {}", plugin.description());
        println!("   Enabled     : {}", plugin.enabled());
        println!("   Interval    : {:?}", plugin.interval());
        println!("   Mode        : {}", plugin.remediation_mode());
        println!("   Remediation : {}", remediation);
        println!("   Status      : {}", status.message);
        println!();
    }

    println!("📄 Config: {}", config_path);

    Ok(())
}

fn format_plan(plan: &[EscalationStep]) -> String {
    if plan.is_empty() {
        return "none".to_string();
    }

    plan.iter()
        .map(|step| {
            let mut s = format!("{}->{}", step.after_failures, action_name(step.action));

            if let Some(service) = &step.service {
                s.push_str(&format!("({})", service));
            }

            if !step.command.is_empty() {
                s.push_str(&format!("({:?})", step.command));
            }

            s
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::None => "none",
        Action::RestartService => "restart_service",
        Action::RunCommand => "run_command",
        Action::Reboot => "reboot",
    }
}
