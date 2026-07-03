Name:           watchguard
Version:        1.0.2
Release:        1%{?dist}
Summary:        Plugin-based host health monitor daemon
License:        MIT
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  systemd-rpm-macros
Requires:       systemd

%description
Watchguard is a Rust-based host health monitor for systemd Linux hosts.

It supports:
- OOM detection through journalctl
- sshd service monitoring through systemd D-Bus
- SSH reachability checks to configurable targets
- outbound TCP network checks
- DNS checks
- boot grace and reboot cooldown failsafes
- CLI plugin management
- doctor diagnostics
- persistent state and remediation history

%prep
%autosetup -n watchguard

%build
cargo build --release

%install
install -Dpm 0755 target/release/watchguard %{buildroot}%{_bindir}/watchguard
install -Dpm 0644 packaging/watchguard.service %{buildroot}/usr/lib/systemd/system/watchguard.service
install -Dpm 0644 packaging/config.toml %{buildroot}%{_sysconfdir}/watchguard/config.toml
install -Dpm 0644 packaging/watchguard.8 %{buildroot}%{_mandir}/man8/watchguard.8
install -dpm 0755 %{buildroot}/var/lib/watchguard

%post
%systemd_post watchguard.service

%preun
%systemd_preun watchguard.service

%postun
%systemd_postun_with_restart watchguard.service

%files
%license LICENSE
%doc README.md docs/README_BUILD.md
%{_bindir}/watchguard
/usr/lib/systemd/system/watchguard.service
%config(noreplace) %{_sysconfdir}/watchguard/config.toml
%{_mandir}/man8/watchguard.8*
%dir /var/lib/watchguard

%changelog
* Fri Jul 03 2026 Watchguard Project <root@localhost> - 1.0.2-1

