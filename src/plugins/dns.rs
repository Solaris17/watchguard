use std::net::SocketAddr;

use trust_dns_client::{
    client::{Client, SyncClient},
    rr::{DNSClass, Name, RecordType},
    udp::UdpClientConnection,
};

use crate::config::DnsConfig;

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
