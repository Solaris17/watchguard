# Watchguard

Watchguard is a Rust-based host health monitor for systemd Linux hosts.

## Features

- OOM detection through `journalctl -kf -n0`
- sshd service-state monitoring through systemd D-Bus
- SSH reachability checks to multiple TCP targets
- outbound network TCP checks
- DNS checks
- boot grace failsafe
- reboot cooldown failsafe
- plugin-style CLI
- `doctor` diagnostics

## Commands

```bash
watchguard status
watchguard doctor
watchguard enable ssh
watchguard disable ssh
watchguard enable network
watchguard enable dns
watchguard enable oom
watchguard config validate
```

## Logs

```bash
journalctl -u watchguard -f
```

## Config

```text
/etc/watchguard/config.toml
```

## Development

```bash
cargo run -- config validate --config ./packaging/config.toml
cargo run -- status --config ./packaging/config.toml
cargo run -- doctor --config ./packaging/config.toml
cargo run -- daemon --config ./packaging/config.toml
```
