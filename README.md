
# Watchguard

Watchguard is a lightweight Rust-based host health monitor for systemd Linux servers.

It is designed to sit close to the host and recover from the kinds of failures that can make a server hard to reach remotely:

- `sshd` service failures
- SSH TCP reachability failures
- outbound network reachability failures
- DNS failures
- Linux OOM events seen in journald

Watchguard runs as a normal systemd service and can restart SSH or reboot the host depending on your configuration.

By default, all monitoring plugins are present in the config but disabled. This keeps a fresh install safe until you explicitly enable the checks you want.

---

## Features

- Single compiled Rust binary
- RPM packaging for RHEL / Rocky / Alma 9
- Manual install support for Arch, Manjaro, Ubuntu, Debian, and other systemd hosts
- Human-readable duration config values such as `500ms`, `5s`, `5m`, and `1h`
- Real plugin trait architecture
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
watchguard logs
watchguard logs --since "1 hour ago"
watchguard logs --boot --no-follow
watchguard version
watchguard config validate
```

Enable plugins:

```bash
sudo watchguard enable ssh
sudo watchguard enable ssh-service
sudo watchguard enable ssh-targets
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

Remove a plugin config table:

```bash
sudo watchguard disable ssh --remove
```

Supported plugin names:

```text
ssh
ssh-service
ssh-targets
network
dns
oom
```

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
    fn fail_limit(&self) -> u32;
    fn failure_action(&self) -> Action;

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

To add a new plugin:

1. Create a new file in `src/plugins/`.
2. Define a plugin struct.
3. Implement the `Plugin` trait.
4. Add it to `src/plugins/mod.rs`.
5. Add it to `src/registry.rs`.

Example:

```rust
pub struct DiskPlugin {
    cfg: DiskConfig,
    state: CheckState,
}

impl Plugin for DiskPlugin {
    fn id(&self) -> &'static str {
        "disk"
    }

    fn name(&self) -> &'static str {
        "Disk"
    }

    fn description(&self) -> &'static str {
        "Monitors disk usage"
    }

    // implement remaining trait methods...
}
```

The daemon does not need hardcoded knowledge of the plugin. It simply iterates over registered `Box<dyn Plugin>` values.

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

By default, every plugin is disabled:

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

Network and DNS actions default to:

```toml
failure_action = "none"
```

Only set them to `reboot` after validating your targets.

---

## Build from source

```bash
cargo fmt
cargo build --release
```

Run from the source tree:

```bash
cargo run -- config validate --config ./packaging/config.toml
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
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

---

## Build a RHEL 9 compatible RPM using Docker

From the directory containing the `watchguard/` project directory:

```bash
tar -czf watchguard-1.0.0.tar.gz watchguard
mkdir -p rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

docker run --rm -it \
  -v "$PWD":/work \
  -w /work \
  rockylinux:9 \
  bash
```

Inside the container:

```bash
dnf install -y rpm-build rust cargo systemd-rpm-macros tar gzip

rpmbuild \
  --define "_topdir /work/rpmbuild" \
  --define "debug_package %{nil}" \
  -ta watchguard-1.0.0.tar.gz

exit
```

The RPM will be created under:

```text
rpmbuild/RPMS/x86_64/
```

---

## Troubleshooting

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

### `watchguard test --all` fails disabled checks

That is expected. `--all` means test configured plugins even if disabled.

Use plain:

```bash
watchguard test
```

to test only enabled plugins.
