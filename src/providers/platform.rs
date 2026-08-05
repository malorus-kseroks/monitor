use crate::domain::{
    Capability, CapabilitySet, HardwareSnapshot, ProbeReport, ProbeState, Snapshot,
};

#[cfg(target_os = "linux")]
use crate::{
    domain::{HardwareMetric, HardwareMetricGroup, MetricSeverity, ServiceEntry},
    privilege,
    providers::{find_trusted_command, run_command},
    sanitize::terminal_text,
};

#[derive(Debug, Clone, Copy)]
pub enum ServiceActionKind {
    Start,
    Stop,
    Restart,
}

#[cfg(target_os = "linux")]
impl ServiceActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[cfg(target_os = "linux")]
pub fn control_service(
    manager: &str,
    service_id: &str,
    action: ServiceActionKind,
) -> Result<String, String> {
    if !valid_service_id(service_id) {
        return Err("service identifier contains unsupported characters".into());
    }
    let verb = action.as_str();
    let (program, args): (&str, Vec<&str>) = match manager {
        "systemd" => ("systemctl", vec![verb, "--", service_id]),
        "openrc" => ("rc-service", vec![service_id, verb]),
        "runit" => ("sv", vec![verb, service_id]),
        "dinit" => ("dinitctl", vec!["--system", verb, service_id]),
        "sysvinit" => ("service", vec![service_id, verb]),
        _ => return Err(format!("unsupported service provider: {manager}")),
    };
    let target = find_trusted_command(program)
        .ok_or_else(|| format!("service command is unavailable: {program}"))?;
    let status = privilege::execute(&target, args).map_err(|error| error.to_string())?;
    if status.success() {
        Ok(format!("{verb} completed for {service_id}"))
    } else {
        Err(format!("{program} exited with status {status}"))
    }
}

#[cfg(not(target_os = "linux"))]
pub fn control_service(
    _manager: &str,
    _service_id: &str,
    _action: ServiceActionKind,
) -> Result<String, String> {
    Err("service control is supported only on Linux".into())
}

#[cfg(any(target_os = "linux", test))]
fn valid_service_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '_' | '.' | ':' | '-'))
}
#[cfg(target_os = "linux")]
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub async fn snapshot(service_override: &str) -> Snapshot<HardwareSnapshot> {
    #[cfg(target_os = "linux")]
    let data = linux_snapshot(service_override).await;
    #[cfg(not(target_os = "linux"))]
    let data = {
        let _ = service_override;
        unsupported_snapshot()
    };
    Snapshot::now(data)
}

#[cfg(not(target_os = "linux"))]
fn unsupported_snapshot() -> HardwareSnapshot {
    HardwareSnapshot {
        provider_reports: vec![report(
            "linux-hardware",
            ProbeState::Unavailable,
            &[Capability::Read],
            Some("Linux-only capability"),
        )],
        ..HardwareSnapshot::default()
    }
}

#[cfg(target_os = "linux")]
async fn linux_snapshot(service_override: &str) -> HardwareSnapshot {
    let mut result = HardwareSnapshot::default();
    result.gpus = scan_gpus();
    #[cfg(feature = "gpu-nvidia")]
    {
        let nvml = scan_nvidia_nvml();
        result.provider_reports.push(report(
            "nvidia-nvml",
            state(!nvml.is_empty()),
            &[Capability::Read],
            None,
        ));
        result.gpus.extend(nvml);
    }
    result.sensors = scan_sensors();
    result.batteries = scan_batteries();
    result.displays = scan_backlights();
    let (manager, services) = scan_services(service_override).await;
    result.services = services;
    result.provider_reports.push(report(
        "gpu-sysfs",
        state(!result.gpus.is_empty()),
        &[Capability::Read],
        None,
    ));
    result.provider_reports.push(report(
        "hwmon",
        state(!result.sensors.is_empty()),
        &[Capability::Read],
        None,
    ));
    result.provider_reports.push(report(
        "power-supply",
        state(!result.batteries.is_empty()),
        &[Capability::Read],
        None,
    ));
    result.provider_reports.push(report(
        "backlight",
        state(!result.displays.is_empty()),
        &[Capability::Read, Capability::Control],
        None,
    ));
    result.provider_reports.push(report(
        &format!("services:{manager}"),
        state(!result.services.is_empty()),
        &[
            Capability::Read,
            Capability::Control,
            Capability::PrivilegedControl,
        ],
        None,
    ));
    for (name, available, caps) in [
        (
            "smartctl",
            find_trusted_command("smartctl").is_some(),
            vec![Capability::Read],
        ),
        (
            "audio",
            find_trusted_command("wpctl").is_some() || find_trusted_command("pactl").is_some(),
            vec![Capability::Read, Capability::Control],
        ),
        (
            "network-manager",
            Path::new("/run/NetworkManager").exists() || find_trusted_command("nmcli").is_some(),
            vec![Capability::Read, Capability::Scan, Capability::Control],
        ),
        (
            "bluez",
            Path::new("/var/run/dbus/system_bus_socket").exists()
                && find_trusted_command("bluetoothctl").is_some(),
            vec![
                Capability::Read,
                Capability::Scan,
                Capability::Pair,
                Capability::Control,
            ],
        ),
    ] {
        result
            .provider_reports
            .push(report(name, state(available), &caps, None));
    }
    result
}

fn report(name: &str, state: ProbeState, caps: &[Capability], reason: Option<&str>) -> ProbeReport {
    ProbeReport {
        provider: name.into(),
        version: None,
        capabilities: caps.iter().copied().collect::<CapabilitySet>(),
        state,
        reason: reason.map(str::to_owned),
    }
}

#[cfg(target_os = "linux")]
fn state(ready: bool) -> ProbeState {
    if ready {
        ProbeState::Ready
    } else {
        ProbeState::Unavailable
    }
}

#[cfg(target_os = "linux")]
fn scan_gpus() -> Vec<HardwareMetricGroup> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("card") || name.contains('-') {
                return None;
            }
            let device = entry.path().join("device");
            if !device.exists() {
                return None;
            }
            let vendor = read_trimmed(device.join("vendor")).unwrap_or_else(|| "unknown".into());
            let model = read_trimmed(device.join("device")).unwrap_or_else(|| "unknown".into());
            let driver = fs::read_link(device.join("driver"))
                .ok()
                .and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "unknown".into());
            let mut metrics = vec![
                metric("Vendor", &vendor),
                metric("Device", &model),
                metric("Driver", &driver),
            ];
            for key in [
                "gpu_busy_percent",
                "mem_busy_percent",
                "mem_info_vram_used",
                "mem_info_vram_total",
            ] {
                if let Some(value) = read_trimmed(device.join(key)) {
                    metrics.push(metric(key, &value));
                }
            }
            Some(HardwareMetricGroup {
                id: canonical_id(&device),
                label: name,
                metrics,
            })
        })
        .collect()
}

#[cfg(all(target_os = "linux", feature = "gpu-nvidia"))]
fn scan_nvidia_nvml() -> Vec<HardwareMetricGroup> {
    use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};

    let Ok(nvml) = Nvml::init() else {
        return vec![];
    };
    let Ok(count) = nvml.device_count() else {
        return vec![];
    };
    (0..count)
        .filter_map(|index| {
            let device = nvml.device_by_index(index).ok()?;
            let name = device
                .name()
                .unwrap_or_else(|_| format!("NVIDIA GPU {index}"));
            let mut metrics = vec![metric("Provider", "NVML")];
            if let Ok(utilization) = device.utilization_rates() {
                metrics.push(metric("GPU usage", &format!("{}%", utilization.gpu)));
                metrics.push(metric("Memory usage", &format!("{}%", utilization.memory)));
            }
            if let Ok(memory) = device.memory_info() {
                metrics.push(metric(
                    "VRAM",
                    &format!("{} / {} bytes", memory.used, memory.total),
                ));
            }
            if let Ok(temperature) = device.temperature(TemperatureSensor::Gpu) {
                metrics.push(metric("Temperature", &format!("{temperature} C")));
            }
            Some(HardwareMetricGroup {
                id: format!("nvml:{index}"),
                label: terminal_text(&name, 256),
                metrics,
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn scan_sensors() -> Vec<HardwareMetric> {
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return vec![];
    };
    let mut metrics = Vec::new();
    for entry in entries.flatten() {
        let base = entry.path();
        let chip = read_trimmed(base.join("name"))
            .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
        let Ok(files) = fs::read_dir(&base) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if !name.starts_with("temp") || !name.ends_with("_input") {
                continue;
            }
            if let Some(raw) = read_trimmed(file.path()).and_then(|value| value.parse::<f64>().ok())
            {
                let celsius = raw / 1000.0;
                metrics.push(HardwareMetric {
                    label: format!("{chip}:{name}"),
                    value: format!("{celsius:.1} °C"),
                    severity: if celsius >= 90.0 {
                        MetricSeverity::Critical
                    } else if celsius >= 75.0 {
                        MetricSeverity::Warning
                    } else {
                        MetricSeverity::Normal
                    },
                });
            }
        }
    }
    metrics
}

#[cfg(target_os = "linux")]
fn scan_batteries() -> Vec<HardwareMetricGroup> {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let base = entry.path();
            if read_trimmed(base.join("type")).as_deref() != Some("Battery") {
                return None;
            }
            let capacity = read_trimmed(base.join("capacity")).unwrap_or_else(|| "unknown".into());
            let status = read_trimmed(base.join("status")).unwrap_or_else(|| "unknown".into());
            Some(HardwareMetricGroup {
                id: canonical_id(&base),
                label: entry.file_name().to_string_lossy().into_owned(),
                metrics: vec![
                    metric("Capacity", &format!("{capacity}%")),
                    metric("Status", &status),
                ],
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn scan_backlights() -> Vec<HardwareMetricGroup> {
    let Ok(entries) = fs::read_dir("/sys/class/backlight") else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let base = entry.path();
            let current = read_trimmed(base.join("actual_brightness"))?;
            let max = read_trimmed(base.join("max_brightness"))?;
            Some(HardwareMetricGroup {
                id: canonical_id(&base),
                label: entry.file_name().to_string_lossy().into_owned(),
                metrics: vec![metric("Brightness", &format!("{current}/{max}"))],
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
async fn scan_services(override_name: &str) -> (String, Vec<ServiceEntry>) {
    let manager = if override_name != "auto" {
        override_name.to_owned()
    } else if Path::new("/run/systemd/system").exists() {
        "systemd".into()
    } else if Path::new("/run/openrc").exists() {
        "openrc".into()
    } else if Path::new("/dev/dinitctl").exists() {
        "dinit".into()
    } else if Path::new("/etc/service").exists() || Path::new("/var/service").exists() {
        "runit".into()
    } else {
        "sysvinit".into()
    };
    let services = match manager.as_str() {
        "systemd" => {
            command_services(
                "systemctl",
                &[
                    "list-units",
                    "--type=service",
                    "--all",
                    "--no-legend",
                    "--no-pager",
                    "--plain",
                ],
            )
            .await
        }
        "openrc" => command_services("rc-status", &["--all"]).await,
        "dinit" => command_services("dinitctl", &["--system", "list"]).await,
        "runit" => directory_services(&[
            "/run/runit/service",
            "/etc/service",
            "/var/service",
            "/service",
        ]),
        _ => directory_services(&["/etc/init.d"]),
    };
    (manager, services)
}

#[cfg(target_os = "linux")]
async fn command_services(program: &str, args: &[&str]) -> Vec<ServiceEntry> {
    let Some(path) = find_trusted_command(program) else {
        return vec![];
    };
    let Ok(output) = run_command(&path, args.iter().copied(), Duration::from_secs(3)).await else {
        return vec![];
    };
    if output.code != Some(0) {
        return vec![];
    }
    output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(500)
        .enumerate()
        .map(|(index, line)| {
            let safe = terminal_text(line.trim(), 512);
            let mut parts = safe.split_whitespace();
            let id = parts
                .next()
                .unwrap_or("unknown")
                .trim_start_matches('*')
                .to_owned();
            ServiceEntry {
                id: id.clone(),
                name: id,
                state: parts.next().unwrap_or("unknown").into(),
                description: parts
                    .collect::<Vec<_>>()
                    .join(" ")
                    .if_empty_then(&format!("service-{index}")),
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn directory_services(roots: &[&str]) -> Vec<ServiceEntry> {
    let Some(root) = roots.iter().map(Path::new).find(|path| path.is_dir()) else {
        return vec![];
    };
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    entries
        .flatten()
        .take(500)
        .map(|entry| {
            let name = terminal_text(&entry.file_name().to_string_lossy(), 256);
            ServiceEntry {
                id: name.clone(),
                name,
                state: "unknown".into(),
                description: root.display().to_string(),
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn metric(label: &str, value: &str) -> HardwareMetric {
    HardwareMetric {
        label: label.into(),
        value: terminal_text(value, 256),
        severity: MetricSeverity::Unknown,
    }
}
#[cfg(target_os = "linux")]
fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| terminal_text(value.trim(), 256))
}
#[cfg(target_os = "linux")]
fn canonical_id(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(target_os = "linux")]
trait EmptyFallback {
    fn if_empty_then(self, fallback: &str) -> String;
}
#[cfg(target_os = "linux")]
impl EmptyFallback for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_identifiers_cannot_inject_arguments_or_paths() {
        assert!(valid_service_id("NetworkManager.service"));
        assert!(valid_service_id("agetty@tty1"));
        assert!(!valid_service_id("--no-password"));
        assert!(!valid_service_id("../../tmp/evil"));
        assert!(!valid_service_id("name;reboot"));
    }
}
