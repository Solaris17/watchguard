# Watchguard Build Instructions

This document covers four build paths:

1. RHEL / Rocky / Alma 9 RPM build
2. Ubuntu build environment
3. Development build
4. Raw binary compile and manual install

Watchguard is written in Rust and does not require a runtime on the target host after compilation.

---

# 1. RHEL / Rocky / Alma 9 RPM Build

This is the recommended build path for producing the final RPM.

## Install build dependencies

```bash
sudo dnf install -y rpm-build rust cargo systemd-rpm-macros tar gzip
```

## Confirm project layout

You should have:

```text
watchguard/
  Cargo.toml
  docs/
    README_BUILD.md
  src/
    main.rs
    cli.rs
    config.rs
    daemon.rs
    status.rs
    doctor.rs
    actions.rs
    util.rs
    plugins/
      mod.rs
      ssh.rs
      network.rs
      dns.rs
      oom.rs
  packaging/
    config.toml
    watchguard.service
    watchguard.8
  README.md
  LICENSE
  watchguard.spec
```

## Create source tarball

From the directory containing `watchguard/`:

```bash
tar -czf watchguard-1.0.0.tar.gz watchguard
```

## Build the RPM

```bash
rpmbuild -ta watchguard-1.0.0.tar.gz
```

The RPM should appear under:

```bash
~/rpmbuild/RPMS/
```

Example:

```bash
~/rpmbuild/RPMS/x86_64/watchguard-1.0.0-1.el9.x86_64.rpm
```

## Install the RPM

```bash
sudo dnf install -y ~/rpmbuild/RPMS/*/watchguard-*.rpm
```

## Enable and start Watchguard

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now watchguard.service
```

## Check status

```bash
watchguard status
watchguard doctor
journalctl -u watchguard -f
```

## Enable plugins

```bash
sudo watchguard enable ssh
sudo watchguard enable network
sudo watchguard enable dns
sudo watchguard enable oom
sudo systemctl restart watchguard.service
```

By default, reboot actions are conservative. Review `/etc/watchguard/config.toml` before setting network or DNS failure actions to `reboot`.

---

# 2. Ubuntu Build Environment

Ubuntu can build the raw binary directly.

For building an RPM from Ubuntu, the recommended method is to use a RHEL-compatible container such as Rocky Linux 9 or AlmaLinux 9. That avoids RPM macro differences between Debian/Ubuntu and RHEL.

## Install Rust build tools on Ubuntu

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config tar gzip
```

Install Rust with rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Load cargo into the current shell:

```bash
source "$HOME/.cargo/env"
```

Check Rust:

```bash
rustc --version
cargo --version
```

## Compile on Ubuntu

From inside the project directory:

```bash
cargo build --release
```

The binary will be:

```bash
target/release/watchguard
```

## Build a RHEL-compatible RPM from Ubuntu using Podman

Install Podman:

```bash
sudo apt update
sudo apt install -y podman
```

From the directory containing `watchguard/`, create the source tarball:

```bash
tar -czf watchguard-1.0.0.tar.gz watchguard
```

Create an RPM output directory on the host:

```bash
mkdir -p rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
```

Start a Rocky Linux 9 build container:

```bash
podman run --rm -it \
  -v "$PWD":/work \
  -w /work \
  rockylinux:9 \
  bash
```

Inside the container, install build dependencies:

```bash
dnf install -y rpm-build rust cargo systemd-rpm-macros tar gzip
```

Inside the container, build the RPM:

```bash
rpmbuild --define "_topdir /work/rpmbuild" -ta watchguard-1.0.0.tar.gz
```

Exit the container:

```bash
exit
```

The RPM will be available on the Ubuntu host under:

```bash
rpmbuild/RPMS/
```

You can then copy the RPM to a RHEL/Rocky/Alma 9 host and install it:

```bash
sudo dnf install -y watchguard-1.0.0-1*.rpm
```

---

# 3. Development Build

Run Watchguard without installing it.

## Validate config

```bash
cargo run -- config validate --config ./packaging/config.toml
```

## Show status

```bash
cargo run -- status --config ./packaging/config.toml
```

## Run doctor

```bash
cargo run -- doctor --config ./packaging/config.toml
```

## Enable SSH plugin in the local packaging config

```bash
cargo run -- enable ssh --config ./packaging/config.toml
```

## Run daemon directly

```bash
sudo cargo run -- daemon --config ./packaging/config.toml
```

The daemon should be run as root if you want it to restart services or reboot the host.

## Recommended developer checks

```bash
cargo fmt
cargo clippy
cargo test
cargo build --release
```

Clean build artifacts:

```bash
cargo clean
```

---

# 4. Raw Binary Compile and Manual Install

Use this when you do not want to build an RPM.

## Compile

From the project directory:

```bash
cargo build --release
```

Optional strip:

```bash
strip target/release/watchguard
```

## Install binary manually

On RHEL/Rocky/Alma:

```bash
sudo install -Dpm 0755 target/release/watchguard /usr/bin/watchguard
```

On Ubuntu, either use `/usr/bin/watchguard` or `/usr/local/bin/watchguard`.

Recommended Ubuntu manual path:

```bash
sudo install -Dpm 0755 target/release/watchguard /usr/local/bin/watchguard
```

## Install config

```bash
sudo install -Dpm 0644 packaging/config.toml /etc/watchguard/config.toml
```

## Install systemd service

For RHEL/Rocky/Alma using `/usr/bin/watchguard`:

```bash
sudo install -Dpm 0644 packaging/watchguard.service /etc/systemd/system/watchguard.service
```

For Ubuntu using `/usr/local/bin/watchguard`, install the service with the path adjusted:

```bash
sudo sed 's|/usr/bin/watchguard|/usr/local/bin/watchguard|g' packaging/watchguard.service \
  | sudo tee /etc/systemd/system/watchguard.service >/dev/null
```

Reload systemd:

```bash
sudo systemctl daemon-reload
```

Enable and start:

```bash
sudo systemctl enable --now watchguard.service
```

Check logs:

```bash
journalctl -u watchguard -f
```

---

# Ubuntu SSH Service Name Note

RHEL-style systems usually use:

```text
sshd.service
```

Ubuntu commonly uses:

```text
ssh.service
```

If running Watchguard on Ubuntu, edit:

```bash
sudo watchguard config edit
```

Change:

```toml
[commands]
restart_ssh = ["/usr/bin/systemctl", "restart", "sshd.service"]

[ssh]
service = "sshd.service"
```

to:

```toml
[commands]
restart_ssh = ["/usr/bin/systemctl", "restart", "ssh.service"]

[ssh]
service = "ssh.service"
```

Then validate:

```bash
watchguard config validate
```

Restart:

```bash
sudo systemctl restart watchguard.service
```

---

# Useful Commands

Validate config:

```bash
watchguard config validate
```

Show config:

```bash
watchguard config show
```

Edit config:

```bash
sudo watchguard config edit
```

Show plugin status:

```bash
watchguard status
```

Run doctor:

```bash
watchguard doctor
```

Enable plugins:

```bash
sudo watchguard enable ssh
sudo watchguard enable network
sudo watchguard enable dns
sudo watchguard enable oom
```

Disable plugins while preserving config:

```bash
sudo watchguard disable ssh
sudo watchguard disable network
sudo watchguard disable dns
sudo watchguard disable oom
```

Disable and remove plugin config:

```bash
sudo watchguard disable ssh --remove
```

Restart service:

```bash
sudo systemctl restart watchguard.service
```

Follow logs:

```bash
journalctl -u watchguard -f
```
