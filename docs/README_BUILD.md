
# Watchguard Build Instructions

This document covers:

1. Development builds
2. Manual install
3. RHEL / Rocky / Alma 9 RPM build
4. Docker-based RPM build from Manjaro, Arch, or Ubuntu
5. Post-install validation

---

## Project layout

Expected source layout:

```text
watchguard/
  Cargo.toml
  Cargo.lock
  build.rs
  README.md
  LICENSE
  watchguard.spec
  docs/
    README_BUILD.md
  packaging/
    config.toml
    watchguard.service
    watchguard.8
  src/
    main.rs
    cli.rs
    config.rs
    daemon.rs
    status.rs
    doctor.rs
    testcmd.rs
    logs.rs
    version.rs
    actions.rs
    util.rs
    plugins/
      mod.rs
      ssh.rs
      network.rs
      dns.rs
      oom.rs
```

---

## Development build

From inside the project directory:

```bash
cargo fmt
cargo build
cargo build --release
```

Binary path:

```text
target/release/watchguard
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

Install dependencies:

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config
```

Install Rust with rustup if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Ubuntu commonly uses `ssh.service` instead of `sshd.service`.

Update config:

```toml
[commands]
restart_ssh = ["/usr/bin/systemctl", "restart", "ssh.service"]

[ssh]
service = "ssh.service"
```

---

## RHEL / Rocky / Alma 9 native RPM build

Install dependencies:

```bash
sudo dnf install -y rpm-build rust cargo systemd-rpm-macros tar gzip
```

From the parent directory containing `watchguard/`:

```bash
tar -czf watchguard-1.0.0.tar.gz watchguard
rpmbuild -ta watchguard-1.0.0.tar.gz
```

RPM output:

```text
~/rpmbuild/RPMS/x86_64/watchguard-1.0.0-1.el9.x86_64.rpm
```

Install:

```bash
sudo dnf install -y ~/rpmbuild/RPMS/x86_64/watchguard-1.0.0-1.el9.x86_64.rpm
sudo systemctl enable --now watchguard.service
```

---

## Docker-based RHEL 9 RPM build from Manjaro, Arch, or Ubuntu

Use this when your workstation is not RHEL 9.

This avoids glibc compatibility problems from building on a newer distro.

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

Install on a RHEL-compatible host:

```bash
sudo dnf install -y watchguard-1.0.0-1.el9.x86_64.rpm
sudo systemctl enable --now watchguard.service
watchguard doctor
```

---

## Rebuild workflow

From repo root:

```bash
cargo fmt
cargo build --release

sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
sudo install -Dpm 0644 packaging/watchguard.service /etc/systemd/system/watchguard.service
sudo systemctl daemon-reload
sudo systemctl restart watchguard.service

watchguard version
watchguard test
watchguard logs --boot --no-follow
```

---

## RPM rebuild workflow

From the parent directory containing `watchguard/`:

```bash
rm -f watchguard-1.0.0.tar.gz
tar -czf watchguard-1.0.0.tar.gz watchguard

docker run --rm -it \
  -v "$PWD":/work \
  -w /work \
  rockylinux:9 \
  bash
```

Inside:

```bash
dnf install -y rpm-build rust cargo systemd-rpm-macros tar gzip

rpmbuild \
  --define "_topdir /work/rpmbuild" \
  --define "debug_package %{nil}" \
  -ta watchguard-1.0.0.tar.gz
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

## Enable plugins

Enable SSH monitoring:

```bash
sudo watchguard enable ssh
sudo systemctl restart watchguard.service
watchguard test
```

Enable network monitoring:

```bash
watchguard test --all
sudo watchguard enable network
sudo systemctl restart watchguard.service
watchguard status
```

Enable DNS monitoring:

```bash
watchguard test --all
sudo watchguard enable dns
sudo systemctl restart watchguard.service
watchguard status
```

Enable OOM monitoring:

```bash
sudo watchguard enable oom
sudo systemctl restart watchguard.service
watchguard status
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

### `%{_unitdir}` is undefined

Use an explicit service path in the spec:

```spec
/usr/lib/systemd/system/watchguard.service
```

### RPM built on Manjaro does not run on RHEL 9

Build inside a Rocky/RHEL/Alma 9 container using Docker.

### systemd warning about `StartLimitIntervalSec`

`StartLimitIntervalSec` and `StartLimitBurst` belong in `[Unit]`, not `[Service]`.

Correct service section:

```ini
[Unit]
Description=Watchguard Host Health Monitor
Documentation=man:watchguard(8)
After=network-online.target
Wants=network-online.target

StartLimitIntervalSec=60
StartLimitBurst=10
```

---

## Useful commands

```bash
watchguard version
watchguard status
watchguard doctor
watchguard test
watchguard test --all
watchguard logs
watchguard logs --since "1 hour ago"
watchguard logs --boot --no-follow
watchguard config validate
watchguard config show
sudo watchguard config edit
```
