
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
- Plugin-style CLI
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

### Status

Shows which plugins are enabled and performs lightweight status checks.

```bash
watchguard status
```

Example:

```text
🛡️  Watchguard status

❌ oom          disabled
❌ ssh          disabled
❌ network      disabled
❌ dns          disabled

📄 Config: /etc/watchguard/config.toml
```

---

### Doctor

Checks config syntax, required commands, systemd/journald availability, and enabled plugin prerequisites.

```bash
watchguard doctor
```

Example:

```text
🛡️  Watchguard Doctor

✅ Config syntax
✅ Config file exists: /etc/watchguard/config.toml
✅ systemctl found
✅ journalctl found
✅ Reboot command configured
✅ SSH restart command configured
ℹ️  SSH service check disabled
ℹ️  SSH target check disabled
ℹ️  Network plugin disabled
ℹ️  DNS plugin disabled
ℹ️  OOM plugin disabled

Overall Health: ✅ OK
```

---

### Test

Runs plugin probes once and exits.

```bash
watchguard test
```

Only enabled plugins are tested.

To test every configured plugin, even disabled ones:

```bash
watchguard test --all
```

This is useful before enabling a plugin. For example, if SSH is disabled but still configured with `sshd.service` and `127.0.0.1:22`, then `watchguard test --all` will still probe those values.

---

### Logs

Follows the Watchguard service logs using `journalctl`.

```bash
watchguard logs
```

Useful variants:

```bash
watchguard logs --since "1 hour ago"
watchguard logs --boot
watchguard logs --boot --no-follow
watchguard logs -n 100
watchguard logs --unit watchguard.service
```

Equivalent manual command:

```bash
journalctl -u watchguard.service -f
```

---

### Version

Shows version and build metadata.

```bash
watchguard version
```

Example:

```text
🛡️  Watchguard

Version    : 1.0.0
Git        : 7ebde4f
Build Unix : 1783100395
Rust       : rustc 1.96.1
Config     : /etc/watchguard/config.toml
```

---

### Config

Create a default config:

```bash
sudo watchguard config init
```

Overwrite an existing config:

```bash
sudo watchguard config init --force
```

Show the config:

```bash
watchguard config show
```

Validate the config:

```bash
watchguard config validate
```

Edit the config with `$EDITOR`, or `vi` if `$EDITOR` is unset:

```bash
sudo watchguard config edit
```

---

### Enable and disable plugins

Enable the whole SSH plugin:

```bash
sudo watchguard enable ssh
sudo systemctl restart watchguard.service
```

Enable only the SSH service-state check:

```bash
sudo watchguard enable ssh-service
sudo systemctl restart watchguard.service
```

Enable only SSH target reachability checks:

```bash
sudo watchguard enable ssh-targets
sudo systemctl restart watchguard.service
```

Enable network monitoring:

```bash
sudo watchguard enable network
sudo systemctl restart watchguard.service
```

Enable DNS monitoring:

```bash
sudo watchguard enable dns
sudo systemctl restart watchguard.service
```

Enable OOM monitoring:

```bash
sudo watchguard enable oom
sudo systemctl restart watchguard.service
```

Disable a plugin while keeping its config:

```bash
sudo watchguard disable ssh
sudo systemctl restart watchguard.service
```

Disable and remove a plugin config section:

```bash
sudo watchguard disable ssh --remove
sudo systemctl restart watchguard.service
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

## Configuration

Default config path:

```text
/etc/watchguard/config.toml
```

Default config source path:

```text
packaging/config.toml
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

---

## Default safety behavior

Watchguard is intentionally conservative after install.

The default config includes every plugin section, but all plugins are disabled:

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

This prevents a new installation from rebooting a host before you validate that the configured targets are reliable.

---

## Global config

```toml
[global]
log_level = "info"

boot_grace_period = "5m"
reboot_cooldown = "30m"

tick = "500ms"
```

### `boot_grace_period`

Suppresses reboot actions for a period after Watchguard starts.

This prevents immediate reboot loops during boot.

### `reboot_cooldown`

Suppresses repeated reboot actions after a reboot attempt.

This helps avoid reboot storms.

### `tick`

Main daemon loop sleep interval.

---

## Commands config

```toml
[commands]
reboot = ["/usr/bin/systemctl", "reboot", "--force", "--force"]
restart_ssh = ["/usr/bin/systemctl", "restart", "sshd.service"]
```

On Ubuntu, the SSH service is often `ssh.service` instead of `sshd.service`:

```toml
[commands]
restart_ssh = ["/usr/bin/systemctl", "restart", "ssh.service"]

[ssh]
service = "ssh.service"
```

---

## OOM plugin

The OOM plugin watches journald kernel logs through:

```bash
journalctl -kf -n0
```

Config:

```toml
[oom]
enabled = false

patterns = [
  "out of memory: kill process",
  "invoked oom-killer",
  "oom-killer",
  "memory cgroup out of memory"
]
```

Enable:

```bash
sudo watchguard enable oom
sudo systemctl restart watchguard.service
```

On match, Watchguard requests the configured reboot action, subject to boot grace and reboot cooldown.

---

## SSH plugin

The SSH plugin has two sub-checks:

1. systemd service-state check
2. TCP target reachability check

Config:

```toml
[ssh]
enabled = false

service_check_enabled = true
service = "sshd.service"
service_check_interval = "2s"
service_fail_limit = 3
service_failure_action = "restart"

target_check_enabled = true
ssh_check_interval = "5s"
ssh_timeout = "1500ms"
ssh_fail_limit = 3
require_all = false

targets = [
  "127.0.0.1:22"
]

ssh_failure_action = "restart"
```

Enable:

```bash
sudo watchguard enable ssh
sudo systemctl restart watchguard.service
```

Test before enabling:

```bash
watchguard test --all
```

If `watchguard test --all` reports:

```text
❌ ssh-service  error: GetUnit
```

then systemd could not find the configured service. Check:

```bash
systemctl status sshd.service
systemctl status ssh.service
systemctl list-unit-files | grep -E '^ssh|^sshd'
```

If `127.0.0.1:22` fails, check whether SSH is listening:

```bash
ss -tlnp | grep ':22'
```

---

## Network plugin

The network plugin performs TCP reachability checks.

Config:

```toml
[network]
enabled = false

check_interval = "5s"
timeout = "1500ms"
fail_limit = 6
require_all = false

targets = [
  "1.1.1.1:443",
  "8.8.8.8:443"
]

failure_action = "none"
```

Enable:

```bash
sudo watchguard enable network
sudo systemctl restart watchguard.service
```

Recommended workflow:

```bash
watchguard test --all
sudo watchguard enable network
sudo systemctl restart watchguard.service
watchguard status
```

Only set `failure_action = "reboot"` after you confirm the targets are reliable for your environment.

---

## DNS plugin

The DNS plugin performs a direct DNS query.

Config:

```toml
[dns]
enabled = false

check_interval = "30s"
fail_limit = 6

server = "1.1.1.1:53"
name = "example.com"

failure_action = "none"
```

Enable:

```bash
sudo watchguard enable dns
sudo systemctl restart watchguard.service
```

Only set `failure_action = "reboot"` after validating the DNS server and query name.

---

## Build from source

Install Rust, then:

```bash
cargo build --release
```

Binary:

```text
target/release/watchguard
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

Install on RHEL / Rocky / Alma 9:

```bash
sudo dnf install -y watchguard-1.0.0-1.el9.x86_64.rpm
sudo systemctl enable --now watchguard.service
watchguard doctor
```

---

## Development notes

Recommended checks:

```bash
cargo fmt
cargo build
cargo build --release
```

Optional if installed:

```bash
cargo clippy
cargo test
```

Commit `Cargo.lock` for application builds.

Suggested `.gitignore`:

```gitignore
/target/
/dist/
/rpmbuild/
*.rpm
*.tar.gz
```

---

## Troubleshooting

### `StartLimitIntervalSec` warning

If systemd prints:

```text
Unknown key 'StartLimitIntervalSec' in section [Service]
```

move these lines to the `[Unit]` section of `watchguard.service`:

```ini
StartLimitIntervalSec=60
StartLimitBurst=10
```

Then reload:

```bash
sudo systemctl daemon-reload
sudo systemctl restart watchguard.service
```

### SSH `GetUnit` error

This usually means the configured SSH service name does not exist.

Check:

```bash
systemctl status sshd.service
systemctl status ssh.service
systemctl list-unit-files | grep -E '^ssh|^sshd'
```

Update `/etc/watchguard/config.toml` accordingly.

### SSH target probe failed

Check whether port 22 is listening:

```bash
ss -tlnp | grep ':22'
```

Install and start OpenSSH server if needed:

Arch / Manjaro:

```bash
sudo pacman -S openssh
sudo systemctl enable --now sshd.service
```

RHEL / Rocky / Alma:

```bash
sudo dnf install -y openssh-server
sudo systemctl enable --now sshd.service
```

Ubuntu:

```bash
sudo apt install -y openssh-server
sudo systemctl enable --now ssh.service
```

### `watchguard test --all` fails disabled checks

That is expected. `--all` means test configured plugins even if disabled.

Use plain:

```bash
watchguard test
```

to test only enabled plugins.
