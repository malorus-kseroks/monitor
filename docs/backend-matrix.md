# Backend and acceptance matrix

This file distinguishes implemented code from runtime acceptance. A provider is
not called production-ready merely because its target compiles.

| Area | Windows | Linux | Current evidence |
|---|---|---|---|
| Overview / processes / filesystems / network | Implemented | Implemented | Windows GNU runtime; four Linux target checks |
| Process filter, sort, details, terminate | Implemented | Implemented | Unit/UI matrix; Windows runtime (no destructive smoke) |
| Disk throughput | Pending PDH backend | `/proc/diskstats` implemented | Parser tests; Linux runtime pending |
| Docker / Podman list and control | Named-pipe code implemented | Unix/Podman code implemented | No local daemon available on test host |
| Container stats and logs | Implemented, bounded | Implemented, bounded | Formula/UI tests; daemon runtime pending |
| Services | Hidden | list + fixed-argv control | Linux runtime/init matrix pending |
| Service logs | Not applicable | Pending follow/filter backend | Release blocker |
| Intel/AMD GPU, hwmon, battery, backlight | Hidden | Read-only sysfs implemented | Hardware smoke pending |
| NVIDIA GPU | Hidden | Dynamic NVML implemented | Compile-only, experimental |
| SMART JSON | Hidden | Capability probe only | Read-only data provider pending |
| Audio / Wi-Fi / Bluetooth | Hidden | Capability probes only | D-Bus/control providers pending |
| Brightness control | Hidden | Read-only sysfs probe | logind/brightnessctl control pending |

## Verified locally

- Rust 1.95 Windows GNU release build and `doctor` runtime;
- `fmt`, `clippy -D warnings`, 20 unit/property/render-matrix tests;
- GNU/musl x86-64/aarch64 target checks;
- RustSec audit, cargo-deny and ShellCheck;
- ConPTY normal exit and Ctrl+C terminal restoration;
- invalid CLI input exits with code 2 before raw mode.

## Release blockers

- Windows MSVC and Windows 10/11 runtime matrix, including PDH;
- Linux GNU/musl runtime across systemd, OpenRC, runit, dinit and SysVinit;
- Docker Desktop and rootless Podman integration runs;
- Dell Latitude 3330 hardware smoke and AMD/NVIDIA hardware validation;
- SMART, service-log, audio, brightness, BlueZ and NetworkManager providers;
- PTY panic/backend-error cases and measured coverage >= 80%;
- credential rotation, all-ref history rewrite and fresh-clone gitleaks scan.
