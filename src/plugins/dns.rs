use anyhow::Result;
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use trust_dns_client::{
    client::{Client, SyncClient},
    rr::{DNSClass, Name, RecordType},
    udp::UdpClientConnection,
};

use crate::{
    config::{AppConfig, DnsConfig, EscalationStep},
    plugin::{CheckState, Plugin, PluginStatus, TickOutcome},
};

pub fn check(cfg: &DnsConfig) -> bool {
    let socket_addr: SocketAddr = match cfg.server.parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    let conn = match UdpClientConnection::new(socket_addr) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let client = SyncClient::new(conn);

    let dns_name = match Name::from_ascii(&cfg.name) {
        Ok(v) => v,
        Err(_) => return false,
    };

    client.query(&dns_name, DNSClass::IN, RecordType::A).is_ok()
}

pub struct DnsPlugin {
    cfg: DnsConfig,
    state: CheckState,
}

impl DnsPlugin {
    pub fn new(cfg: &AppConfig) -> Self {
        Self {
            cfg: cfg.dns.clone(),
            state: CheckState::default(),
        }
    }
}

impl Plugin for DnsPlugin {
    fn id(&self) -> &'static str {
        "dns"
    }

    fn name(&self) -> &'static str {
        "DNS"
    }

    fn description(&self) -> &'static str {
        "Monitors DNS resolution"
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn interval(&self) -> Duration {
        self.cfg.check_interval
    }

    fn escalation_steps(&self) -> Vec<EscalationStep> {
        self.cfg.failure_actions.clone()
    }

    fn failure_reason(&self) -> &'static str {
        "DNS failure limit exceeded"
    }

    fn success_message(&self) -> &'static str {
        "DNS recovered"
    }

    fn update_config(&mut self, cfg: &AppConfig) {
        self.cfg = cfg.dns.clone();
    }

    fn probe(&mut self, _rt: &Runtime) -> Result<bool> {
        Ok(check(&self.cfg))
    }

    fn status(&mut self, rt: &Runtime) -> PluginStatus {
        if !self.enabled() {
            return PluginStatus::disabled(self.id(), "disabled");
        }

        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!("{} via {}", self.cfg.name, self.cfg.server),
            ),
            Ok(false) => PluginStatus::warning(
                self.id(),
                format!(
                    "DNS probe failed: {} via {}",
                    self.cfg.name, self.cfg.server
                ),
            ),
            Err(e) => PluginStatus::warning(self.id(), format!("status error: {:#}", e)),
        }
    }

    fn test(&mut self, rt: &Runtime) -> PluginStatus {
        match self.probe(rt) {
            Ok(true) => PluginStatus::healthy(
                self.id(),
                format!("{} via {}", self.cfg.name, self.cfg.server),
            ),
            Ok(false) => PluginStatus::failed(
                self.id(),
                format!("failed: {} via {}", self.cfg.name, self.cfg.server),
            ),
            Err(e) => PluginStatus::failed(self.id(), format!("error: {:#}", e)),
        }
    }

    fn tick(&mut self, rt: &Runtime, now: Instant) -> TickOutcome {
        if !self.enabled() {
            self.state.reset_disabled(now);
            return TickOutcome::Idle;
        }

        if !self.state.due(now, self.interval()) {
            return TickOutcome::Idle;
        }

        let result = self.probe(rt);
        let escalation_steps = self.escalation_steps();

        self.state.record(
            self.id(),
            &escalation_steps,
            self.failure_reason(),
            self.success_message(),
            result,
        )
    }
}
