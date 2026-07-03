use crate::{
    config::AppConfig,
    plugin::Plugin,
    plugins::{
        dns::DnsPlugin,
        network::NetworkPlugin,
        oom::OomPlugin,
        ssh::{SshServicePlugin, SshTargetsPlugin},
    },
};

pub fn build_plugins(cfg: &AppConfig) -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(OomPlugin::new(cfg)),
        Box::new(SshServicePlugin::new(cfg)),
        Box::new(SshTargetsPlugin::new(cfg)),
        Box::new(NetworkPlugin::new(cfg)),
        Box::new(DnsPlugin::new(cfg)),
    ]
}
