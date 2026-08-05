# Architecture

The binary owns terminal lifecycle and delegates state acquisition to a bounded
supervisor. Providers publish immutable timestamped snapshots. UI rendering only
reads state and never performs I/O.

Provider states are explicit: Loading, Ready, Empty, Degraded, Unavailable,
PermissionDenied, Error and Stale. Stable identifiers are used for processes and
containers so actions cannot silently target a different object after refresh.

## Providers

- Portable: Sysinfo + if-addrs; Bollard Docker-compatible API.
- Linux: sysfs for DRM/hwmon/power_supply/backlight; init-specific service probes;
  runtime capability detection for smartctl, PipeWire/PulseAudio,
  NetworkManager and BlueZ.
- Windows: Sysinfo/Win32-backed telemetry and local Docker-compatible named pipe.

Provider refreshes have bounded queues, skip missed ticks and cap command output
at 1 MiB. The application never executes arbitrary user-provided commands.
Docker stats are bounded to 16 concurrent running containers and log views to
500 sanitized lines / 1 MiB. Linux disk rates are derived from `/proc/diskstats`.
Service identifiers are allow-listed before fixed-argv invocation; privileged
control suspends the TUI and gives the inherited TTY directly to `sudo`/`doas`.

## Deliberate boundaries

No service enable/disable, disk writes, SMART tests, GPU controls, DDC, container
delete/prune/build/exec, firewall, package management or log deletion.
