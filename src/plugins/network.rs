use crate::{config::NetworkConfig, util};

pub fn check(cfg: &NetworkConfig) -> bool {
    util::multi_target_probe(&cfg.targets, cfg.require_all, cfg.timeout)
}
