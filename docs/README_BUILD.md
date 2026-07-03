
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
cargo run -- status --config ./packaging/config.toml
cargo run -- doctor --config ./packaging/config.toml
cargo run -- test --config ./packaging/config.toml
cargo run -- test --all --config ./packaging/config.toml
```

Run daemon manually:

```bash
sudo cargo run -- daemon --config ./packaging/config.toml
```

---

## Plugin architecture

Core files:

```text
src/plugin.rs
src/registry.rs
src/plugins/
```

`src/plugin.rs` defines:

```text
Plugin
PluginStatus
Health
CheckState
TickOutcome
```

`src/registry.rs` owns plugin registration:

```rust
pub fn build_plugins(cfg: &AppConfig) -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(OomPlugin::new(cfg)),
        Box::new(SshServicePlugin::new(cfg)),
        Box::new(SshTargetsPlugin::new(cfg)),
        Box::new(NetworkPlugin::new(cfg)),
        Box::new(DnsPlugin::new(cfg)),
    ]
}
```

To add a new health check:

1. Add the config struct in `src/config.rs`.
2. Add the TOML defaults in `packaging/config.toml`.
3. Add a plugin module under `src/plugins/`.
4. Implement the `Plugin` trait.
5. Register it in `src/registry.rs`.
6. Add CLI enable/disable support if it should be user-toggleable.

---

## Manual install

Build:

```bash
cargo build --release
```

Install binary, config, and service:

```bash
sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
sudo install -Dpm 0644 packaging/config.toml /etc/watchguard/config.toml
sudo install -Dpm 0644 packaging/watchguard.service /etc/systemd/system/watchguard.service
```

Enable service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now watchguard.service
```

Validate:

```bash
watchguard version
watchguard doctor
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

Restart after reinstalling the binary:

```bash
sudo systemctl restart watchguard.service
```

---

## Manjaro / Arch notes

Install dependencies:

```bash
sudo pacman -S --needed rust cargo base-devel
```

If you want SSH checks to pass locally:

```bash
sudo pacman -S openssh
sudo systemctl enable --now sshd.service
```

Check:

```bash
systemctl status sshd.service
ss -tlnp | grep ':22'
```

---

## Ubuntu notes

Ubuntu commonly uses `ssh.service` instead of `sshd.service`.

Update config:

```toml
[commands]
restart_ssh = ["/usr/bin/systemctl", "restart", "ssh.service"]

[ssh]
service = "ssh.service"
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

Back on the host:

```bash
ls -lh rpmbuild/RPMS/x86_64/
```

---

## Post-install checks

```bash
systemctl status watchguard.service
watchguard version
watchguard doctor
watchguard status
watchguard test
watchguard logs --boot --no-follow
```

Follow logs:

```bash
watchguard logs
```

---

## Rebuild workflow

From repo root:

```bash
cargo fmt
cargo build --release

sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
sudo systemctl restart watchguard.service

watchguard version
watchguard test
watchguard logs --boot --no-follow
```

---

## Troubleshooting build issues

### `debugsourcefiles.list` is empty

Use the build-time RPM define:

```bash
rpmbuild \
  --define "_topdir /work/rpmbuild" \
  --define "debug_package %{nil}" \
  -ta watchguard-1.0.0.tar.gz
```

### RPM built on Manjaro does not run on RHEL 9

Build inside a Rocky/RHEL/Alma 9 container using Docker.

### systemd warning about `StartLimitIntervalSec`

`StartLimitIntervalSec` and `StartLimitBurst` belong in `[Unit]`, not `[Service]`.
