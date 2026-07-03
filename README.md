# Watchguard

Watchguard is a lightweight Rust-based host health monitor for systemd Linux servers.

It is designed to sit close to the host and recover from failures that can make a server hard to reach remotely:

- `sshd` service failures
- SSH TCP reachability failures
- outbound network reachability failures
- DNS failures
- Linux OOM events seen in journald

Watchguard runs as a normal systemd service and can restart services, run commands, or reboot the host depending on the plugin remediation policy.

By default, all monitoring plugins are present in the config but disabled. Network and DNS use safe no-op escalation defaults.

---

## Features

- Single compiled Rust binary
- RPM packaging for RHEL / Rocky / Alma 9
- Manual install support for Arch, Manjaro, Ubuntu, Debian, and other systemd hosts
- Human-readable duration config values such as `500ms`, `5s`, `5m`, and `1h`
- Real plugin trait architecture
- Generic action engine
- Escalation for polling checks, plus immediate event-driven OOM reboot
- `watchguard plugins` plugin metadata and remediation display
- `watchguard state` persistent state summary
- `watchguard history` persistent remediation history
- `watchguard doctor` diagnostics
- `watchguard test` live one-shot probe checks
- `watchguard logs` journal helper
- `watchguard version` build metadata
- Boot grace period to avoid rebooting immediately after startup
- Reboot cooldown to avoid reboot storms
- OOM detection through `journalctl -kf -n0`
- SSH service-state checks through systemd D-Bus
- TCP reachability probes for SSH and network targets
- DNS probe support

---

## Quick start

After installing Watchguard:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now watchguard.service
```

Check the install:

```bash
watchguard doctor
watchguard plugins
watchguard state
watchguard history
watchguard status
watchguard test
```

View logs:

```bash
watchguard logs
```

---

## Commands

```bash
watchguard status
watchguard doctor
watchguard test
watchguard test --all
watchguard plugins
watchguard state
watchguard history
watchguard logs
watchguard logs --since "1 hour ago"
watchguard logs --boot --no-follow
watchguard version
watchguard config validate
```

Enable plugins:

```bash
sudo watchguard enable ssh
sudo watchguard enable network
sudo watchguard enable dns
sudo watchguard enable oom
sudo systemctl restart watchguard.service
```

Disable plugins:

```bash
sudo watchguard disable ssh
sudo watchguard disable network
sudo watchguard disable dns
sudo watchguard disable oom
sudo systemctl restart watchguard.service
```

---

## Action engine

Watchguard has one generic action engine. Polling checks use escalation steps. OOM is event-driven and immediately requests reboot on the first matched kernel OOM journal event.

Supported actions:

| Action | Purpose |
|---|---|
| `none` | Log only; take no remediation |
| `restart_service` | Restart a configured systemd service |
| `run_command` | Run a configured command vector |
| `reboot` | Run `[commands].reboot`, subject to boot grace and reboot cooldown |

### `none`

Safe no-op action:

```toml
{ after_failures = 6, action = "none" }
```

### `restart_service`

Generic service restart:

```toml
{ after_failures = 3, action = "restart_service", service = "sshd.service" }
```

This runs:

```bash
/usr/bin/systemctl restart sshd.service
```

### `run_command`

Generic command execution:

```toml
{ after_failures = 3, action = "run_command", command = ["/usr/bin/systemctl", "restart", "systemd-resolved.service"] }
```

### `reboot`

Reboot action:

```toml
{ after_failures = 9, action = "reboot" }
```

The reboot action is protected by:

```toml
[global]
boot_grace_period = "5m"
reboot_cooldown = "30m"
```

---

## Escalation

Polling checks have a `failure_actions` list or a check-specific equivalent such as `service_failure_actions`. OOM does not use `failure_actions`; it is an event-driven immediate reboot policy.

Example:

```toml
failure_actions = [
  { after_failures = 3, action = "restart_service", service = "example.service" },
  { after_failures = 6, action = "run_command", command = ["/usr/local/bin/fix-example"] },
  { after_failures = 9, action = "reboot" }
]
```

Behavior:

```text
3 consecutive failures -> restart service
6 consecutive failures -> run command
9 consecutive failures -> reboot
success -> reset counter
```

After the final escalation step has fired, Watchguard repeats the final step periodically using the final step threshold. For example, a final step at 9 failures repeats at 18, 27, and so on while the failure remains unresolved.

---

## SSH escalation defaults

The packaged SSH defaults are:

```toml
service_failure_actions = [
  { after_failures = 3, action = "restart_service", service = "sshd.service" },
  { after_failures = 6, action = "restart_service", service = "sshd.service" },
  { after_failures = 9, action = "reboot" }
]

ssh_failure_actions = [
  { after_failures = 3, action = "restart_service", service = "sshd.service" },
  { after_failures = 6, action = "restart_service", service = "sshd.service" },
  { after_failures = 9, action = "reboot" }
]
```

Since SSH is disabled by default, this only applies after enabling SSH monitoring:

```bash
sudo watchguard enable ssh
sudo systemctl restart watchguard.service
```

---

## Network and DNS safe defaults

Network and DNS remain conservative by default:

```toml
failure_actions = [
  { after_failures = 6, action = "none" }
]
```

Example network escalation:

```toml
[network]
enabled = true

failure_actions = [
  { after_failures = 6, action = "restart_service", service = "NetworkManager.service" },
  { after_failures = 12, action = "reboot" }
]
```

Example DNS escalation:

```toml
[dns]
enabled = true

failure_actions = [
  { after_failures = 3, action = "run_command", command = ["/usr/bin/systemctl", "restart", "systemd-resolved.service"] },
  { after_failures = 9, action = "reboot" }
]
```

Only set reboot actions after validating your targets.

---

## OOM immediate reboot policy

OOM is intentionally different from SSH, network, and DNS checks. It is event-driven instead of failure-count driven.

```toml
[oom]
enabled = true
debounce = "5s"

patterns = [
  "out of memory: kill process",
  "invoked oom-killer",
  "oom-killer",
  "memory cgroup out of memory"
]
```

When Watchguard sees one of those journal patterns, it immediately requests a reboot. The request is still protected by the global safety guards:

```toml
[global]
boot_grace_period = "5m"
reboot_cooldown = "30m"
```

There is no `failure_actions` list for OOM because an OOM event means the kernel has already killed a process and the host may be partially degraded.

---

## Persistent state database

Watchguard keeps a small JSON state database at:

```text
/var/lib/watchguard/state.json
```

It records remediation events and summary counters, including:

- plugin name
- action
- status
- reason
- command/service
- failure count and threshold
- reboot requests
- OOM event count

View the state summary:

```bash
watchguard state
```

View remediation history:

```bash
watchguard history
watchguard history -n 50
```

The state file is written before reboot actions are executed, so a Watchguard-initiated reboot should still leave a record after the machine comes back up. The systemd unit also uses:

```ini
StateDirectory=watchguard
```

so systemd creates `/var/lib/watchguard` automatically when the service starts.


---

## Plugin architecture

Watchguard uses a Rust trait for health checks:

```rust
pub trait Plugin {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    fn enabled(&self) -> bool;
    fn interval(&self) -> Duration;
    fn escalation_steps(&self) -> Vec<EscalationStep>;
    fn remediation_mode(&self) -> &'static str;
    fn remediation_summary(&self) -> Option<String>;

    fn update_config(&mut self, cfg: &AppConfig);
    fn probe(&mut self, rt: &Runtime) -> Result<bool>;

    fn status(&mut self, rt: &Runtime) -> PluginStatus;
    fn doctor(&mut self, rt: &Runtime) -> Vec<PluginStatus>;
    fn test(&mut self, rt: &Runtime) -> PluginStatus;
    fn tick(&mut self, rt: &Runtime, now: Instant) -> TickOutcome;
}
```

Plugins are registered in:

```text
src/registry.rs
```

Current plugins:

```text
OomPlugin
SshServicePlugin
SshTargetsPlugin
NetworkPlugin
DnsPlugin
```

---

## Configuration

Default config path:

```text
/etc/watchguard/config.toml
```

Durations are human-readable:

```text
500ms
2s
30s
1m
5m
30m
1h
```

By default, every plugin is disabled. OOM has no `failure_actions` field; enabling it activates immediate reboot-on-OOM-event behavior:

```toml
[oom]
enabled = false

[ssh]
enabled = false

[network]
enabled = false

[dns]
enabled = false
```

---

## Build from source

```bash
cargo fmt
cargo build --release
```

Run from the source tree:

```bash
cargo run -- config validate --config ./packaging/config.toml
cargo run -- plugins --config ./packaging/config.toml
cargo run -- state
cargo run -- history
cargo run -- status --config ./packaging/config.toml
cargo run -- doctor --config ./packaging/config.toml
cargo run -- test --config ./packaging/config.toml
cargo run -- test --all --config ./packaging/config.toml
```

---

## Manual install

```bash
cargo build --release

sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
sudo install -Dpm 0644 packaging/config.toml /etc/watchguard/config.toml
sudo install -Dpm 0644 packaging/watchguard.service /etc/systemd/system/watchguard.service

sudo systemctl daemon-reload
sudo systemctl enable --now watchguard.service
```

Verify:

```bash
watchguard doctor
watchguard plugins
watchguard state
watchguard history
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

---

## Troubleshooting

### `watchguard test --all` fails disabled checks

That is expected. `--all` means test configured plugins even if disabled.

Use plain:

```bash
watchguard test
```

to test only enabled plugins.

### SSH `GetUnit` error

This usually means the configured SSH service name does not exist.

Check:

```bash
systemctl status sshd.service
systemctl status ssh.service
systemctl list-unit-files | grep -E '^ssh|^sshd'
```

### SSH target probe failed

Check whether port 22 is listening:

```bash
ss -tlnp | grep ':22'
```

---

## Logging

Watchguard logs to journald through the `watchguard.service` unit.

View logs:

```bash
watchguard logs
watchguard logs --boot --no-follow
journalctl -u watchguard.service -f
```

The daemon logs:

- daemon startup and global timing settings
- plugin registration and enabled state
- plugin failures, thresholds, and recoveries
- OOM journal watcher start/stop/restart
- matched OOM journal lines with duplicate OOM messages debounced
- remediation decisions
- remediation command start, success, and failure
- persistent state writes to `/var/lib/watchguard/state.json`
- reboot suppression due to boot grace or reboot cooldown

Successful checks are not logged on every tick by default to avoid log spam. Increase detail with:

```toml
[global]
log_level = "debug"
```
