# Watchguard Build Instructions

## Development build

```bash
cargo fmt
cargo build
cargo build --release
```

Run commands from source:

```bash
cargo run -- version
cargo run -- config validate --config ./packaging/config.toml
cargo run -- plugins --config ./packaging/config.toml
cargo run -- status --config ./packaging/config.toml
cargo run -- doctor --config ./packaging/config.toml
cargo run -- test --config ./packaging/config.toml
cargo run -- test --all --config ./packaging/config.toml
```

---

## Action engine

The action engine is shared by escalation-driven polling plugins and event-driven plugins such as OOM.

Action types:

```text
none
restart_service
run_command
reboot
```

The action engine lives in:

```text
src/actions.rs
```

Action and escalation configuration types live in:

```text
src/config.rs
```

Escalation state and threshold logic for polling checks lives in:

```text
src/plugin.rs
```

---

## Escalation development notes

Escalation is represented by:

```rust
pub struct EscalationStep {
    pub after_failures: u32,
    pub action: Action,
    pub service: Option<String>,
    pub command: Vec<String>,
}
```

Polling checks use escalation lists:

```toml
failure_actions = [
  { after_failures = 3, action = "restart_service", service = "example.service" },
  { after_failures = 6, action = "run_command", command = ["/usr/local/bin/fix-example"] },
  { after_failures = 9, action = "reboot" }
]
```

The plugin failure counter resets only when the probe succeeds. OOM is event-driven and does not use failure counters or `failure_actions`; it immediately requests reboot on the first matched OOM journal event, subject to boot grace and reboot cooldown.

After the final escalation step has fired, Watchguard repeats the final step periodically using the final step threshold. For example, a final step at 9 failures repeats at 18, 27, and so on while the failure remains unresolved.

---

## Plugin architecture

Core files:

```text
src/plugin.rs
src/registry.rs
src/plugins/
```

To add a new health check:

1. Add the config struct in `src/config.rs`.
2. Add TOML defaults in `packaging/config.toml`.
3. Add a plugin module under `src/plugins/`.
4. Implement the `Plugin` trait.
5. Register it in `src/registry.rs`.
6. Add CLI enable/disable support if it should be user-toggleable.

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

Validate:

```bash
watchguard version
watchguard plugins
watchguard doctor
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

---

## Docker-based RHEL 9 RPM build from Manjaro, Arch, or Ubuntu

From the parent directory containing `watchguard/`:

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

---

## Post-install checks

```bash
systemctl status watchguard.service
watchguard version
watchguard plugins
watchguard doctor
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

---

## Rebuild workflow

```bash
cargo fmt
cargo build --release

sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
sudo systemctl restart watchguard.service

watchguard version
watchguard plugins
watchguard test
watchguard logs --boot --no-follow
```

---

## Logging checks

After starting the service:

```bash
watchguard logs --boot --no-follow
journalctl -u watchguard.service -f
```

You should see daemon startup, plugin registration, OOM watcher state, plugin failures/recoveries, and remediation actions.

Successful probes are intentionally not logged every tick at `info` level.
