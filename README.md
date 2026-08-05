# KernOX Monitor

KernOX Monitor is a capability-aware terminal system monitor written in Rust.
It provides a portable core for Windows and Linux and exposes Linux workstation
features only when a suitable provider is actually available.

> Security notice: this repository previously committed a local sudo password.
> Treat that password and the former embedded GitLab token as compromised.
> The public history must be rewritten before any release is published.

## Platform contract

| Page | Windows 10/11 | Linux |
|---|---|---|
| Overview, Processes, Storage, Network | Supported | Supported |
| Docker/Podman | Local named pipe | Local Unix sockets |
| Services and logs | Hidden/diagnostic only | systemd, OpenRC, runit, dinit, SysVinit |
| Hardware | Hidden/diagnostic only | GPU, hwmon, battery, backlight and provider probes |

Linux-only pages are hidden on Windows unless `--show-unsupported` or
`show_unsupported_modules = true` is set.

## Build

Rust 1.95 or newer is required. KernOX Monitor never bootstraps Rust itself.

```sh
cargo build --release --locked
./target/release/kernox-monitor doctor
./target/release/kernox-monitor
```

Use `./install.sh --user` for a rootless install or `./install.sh --system` for
`/usr/local/bin`. Use `./uninstall.sh` to remove it.

## Controls

- `Tab` / `Shift+Tab`, `1..7`: pages
- `Up/Down`, `PgUp/PgDn`, `Home`: list navigation
- `Enter`, `c/m/p/n`: process details and CPU/RAM/PID/name sorting
- `/`: process search
- `k` / `Shift+K`: terminate / force terminate after `y` confirmation
- `Enter`, `t`, `s`, `r` on Containers: logs, start, stop, restart
- `t`, `s`, `r` on Services: start, stop, restart through inherited `sudo`/`doas` TTY
- `F5`: refresh; `l`: language; `?`: help; `q` or `Ctrl+C`: quit

## Configuration

The strict v1 TOML config is read from the platform config directory and is not
created automatically:

```toml
version = 1
language = "auto"
interval = "500ms"
default_page = "overview"
color = "auto"
ascii = false
show_unsupported_modules = false

[containers]
endpoints = []

[services]
provider = "auto"
```

Precedence is CLI, `KERNOX_MONITOR_*` / `DOCKER_HOST`, config, defaults. Plain
remote Docker TCP endpoints are rejected.

## Safety model

The program never reads or stores sudo passwords, never invokes `sh -c`, and
does not put Wi-Fi credentials in argv or logs. Disruptive actions require a
separate `y`. Delete/prune/format/package/firewall operations are out of scope.

The 0.2.0 working tree passes Windows GNU unit/clippy/audit/deny checks and
cross-target Linux compilation. Public release remains blocked until Linux and
Windows runtime matrices, PTY cleanup, coverage, real hardware smoke tests and
historical secret remediation are complete; see [release procedure](docs/release.md).

See [architecture](docs/architecture.md), [backend matrix](docs/backend-matrix.md), [security](SECURITY.md), and
[Russian](docs/README.ru.md) / [Ukrainian](docs/README.uk.md) guides.
