use async_trait::async_trait;
use if_addrs::get_if_addrs;
use std::{collections::HashMap, path::Path, time::Instant};
use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, Signal, System,
};

use crate::{
    domain::{
        ActionReceipt, Capability, CapabilitySet, CpuSnapshot, NetworkEntry, ProbeReport,
        ProbeState, ProcessEntry, Provider, ProviderError, Snapshot, StorageEntry, SystemSnapshot,
    },
    sanitize::terminal_text,
};

#[derive(Debug, Clone)]
pub enum SystemAction {
    Terminate {
        pid: u32,
        start_time: u64,
        force: bool,
    },
    Refresh,
}

pub struct SystemProvider {
    system: System,
    disks: Disks,
    networks: Networks,
    last_network_refresh: Instant,
    last_disk_refresh: Instant,
    last_disk_counters: HashMap<String, (u64, u64)>,
}

impl Default for SystemProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProvider {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        );
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last_network_refresh: Instant::now(),
            last_disk_refresh: Instant::now(),
            last_disk_counters: HashMap::new(),
        }
    }

    fn collect(&mut self) -> SystemSnapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh(true);
        let disk_elapsed = self.last_disk_refresh.elapsed().as_secs_f64().max(0.001);
        let current_disk_counters = disk_counters();
        let disk_rates = current_disk_counters
            .iter()
            .filter_map(|(name, (read, written))| {
                let previous = self.last_disk_counters.get(name)?;
                Some((
                    name.clone(),
                    (
                        read.saturating_sub(previous.0) as f64 / disk_elapsed,
                        written.saturating_sub(previous.1) as f64 / disk_elapsed,
                    ),
                ))
            })
            .collect::<HashMap<_, _>>();
        self.last_disk_counters = current_disk_counters;
        self.last_disk_refresh = Instant::now();
        let elapsed = self.last_network_refresh.elapsed().as_secs_f64().max(0.001);
        self.networks.refresh(true);
        self.last_network_refresh = Instant::now();

        let cpus = self.system.cpus();
        let total_percent = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(sysinfo::Cpu::cpu_usage).sum::<f32>() / cpus.len() as f32
        };
        let frequency_mhz = cpus.first().map_or(0, sysinfo::Cpu::frequency);
        let mut processes = self
            .system
            .processes()
            .values()
            .map(|process| ProcessEntry {
                pid: process.pid().as_u32(),
                start_time: process.start_time(),
                name: terminal_text(&process.name().to_string_lossy(), 256),
                command: terminal_text(
                    &process
                        .cmd()
                        .iter()
                        .map(|item| item.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" "),
                    1024,
                ),
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory(),
                status: format!("{:?}", process.status()),
            })
            .collect::<Vec<_>>();
        processes.sort_by(|left, right| right.cpu_percent.total_cmp(&left.cpu_percent));

        let storage = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                let device_name = Path::new(disk.name())
                    .file_name()
                    .unwrap_or_else(|| disk.name())
                    .to_string_lossy();
                let rates = disk_rates.get(device_name.as_ref()).copied();
                StorageEntry {
                    id: format!(
                        "{}:{}",
                        disk.name().to_string_lossy(),
                        disk.mount_point().display()
                    ),
                    name: terminal_text(&disk.name().to_string_lossy(), 128),
                    mount_point: terminal_text(&disk.mount_point().to_string_lossy(), 256),
                    file_system: terminal_text(&disk.file_system().to_string_lossy(), 64),
                    total_bytes: disk.total_space(),
                    available_bytes: disk.available_space(),
                    removable: disk.is_removable(),
                    read_per_second: rates.map(|value| value.0),
                    write_per_second: rates.map(|value| value.1),
                }
            })
            .collect();

        let mut addresses: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(items) = get_if_addrs() {
            for item in items {
                let address = item.ip().to_string();
                addresses.entry(item.name).or_default().push(address);
            }
        }
        let mut networks = self
            .networks
            .iter()
            .map(|(name, network)| NetworkEntry {
                name: terminal_text(name, 128),
                addresses: addresses.remove(name).unwrap_or_default(),
                received_per_second: network.received() as f64 / elapsed,
                transmitted_per_second: network.transmitted() as f64 / elapsed,
                total_received: network.total_received(),
                total_transmitted: network.total_transmitted(),
            })
            .collect::<Vec<_>>();
        networks.sort_by(|a, b| a.name.cmp(&b.name));

        let load = System::load_average();
        SystemSnapshot {
            host_name: System::host_name().unwrap_or_else(|| "unknown".into()),
            os_name: System::long_os_version()
                .or_else(System::name)
                .unwrap_or_else(|| std::env::consts::OS.into()),
            kernel_version: System::kernel_version().unwrap_or_else(|| "unknown".into()),
            uptime_seconds: System::uptime(),
            load_average: cfg!(target_os = "linux").then_some([load.one, load.five, load.fifteen]),
            cpu: CpuSnapshot {
                total_percent,
                per_core_percent: cpus.iter().map(sysinfo::Cpu::cpu_usage).collect(),
                frequency_mhz,
            },
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            processes,
            storage,
            networks,
        }
    }
}

#[cfg(target_os = "linux")]
fn disk_counters() -> HashMap<String, (u64, u64)> {
    std::fs::read_to_string("/proc/diskstats")
        .map(|contents| parse_diskstats(&contents))
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
fn disk_counters() -> HashMap<String, (u64, u64)> {
    HashMap::new()
}

#[cfg(any(target_os = "linux", test))]
fn parse_diskstats(contents: &str) -> HashMap<String, (u64, u64)> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 10 {
                return None;
            }
            let sectors_read = fields[5].parse::<u64>().ok()?;
            let sectors_written = fields[9].parse::<u64>().ok()?;
            Some((
                fields[2].to_owned(),
                (
                    sectors_read.saturating_mul(512),
                    sectors_written.saturating_mul(512),
                ),
            ))
        })
        .collect()
}

#[async_trait]
impl Provider for SystemProvider {
    type Data = SystemSnapshot;
    type Action = SystemAction;

    async fn probe(&mut self) -> ProbeReport {
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(Capability::Read);
        capabilities.insert(Capability::Control);
        ProbeReport {
            provider: "sysinfo".into(),
            version: None,
            capabilities,
            state: ProbeState::Ready,
            reason: None,
        }
    }

    async fn snapshot(&mut self) -> Result<Snapshot<Self::Data>, ProviderError> {
        Ok(Snapshot::now(self.collect()))
    }

    async fn execute(&mut self, action: Self::Action) -> Result<ActionReceipt, ProviderError> {
        match action {
            SystemAction::Refresh => {
                self.collect();
                Ok(receipt("system", "refreshed"))
            }
            SystemAction::Terminate {
                pid,
                start_time,
                force,
            } => {
                if start_time == 0 {
                    return Err(ProviderError::Unavailable(
                        "process start time is unavailable; refusing an unstable PID-only action"
                            .into(),
                    ));
                }
                self.system.refresh_processes(ProcessesToUpdate::All, true);
                let pid = sysinfo::Pid::from_u32(pid);
                let process = self
                    .system
                    .process(pid)
                    .ok_or_else(|| ProviderError::Unavailable("process no longer exists".into()))?;
                if process.start_time() != start_time {
                    return Err(ProviderError::Unavailable(
                        "PID was reused; action cancelled".into(),
                    ));
                }
                let result = if force {
                    process.kill()
                } else {
                    process
                        .kill_with(Signal::Term)
                        .unwrap_or_else(|| process.kill())
                };
                if !result {
                    return Err(ProviderError::PermissionDenied(format!(
                        "cannot terminate PID {}",
                        pid.as_u32()
                    )));
                }
                Ok(receipt(
                    &pid.as_u32().to_string(),
                    if force {
                        "force terminated"
                    } else {
                        "terminated"
                    },
                ))
            }
        }
    }
}

fn receipt(target: &str, summary: &str) -> ActionReceipt {
    ActionReceipt {
        target: target.into(),
        summary: summary.into(),
        completed_at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_diskstats_sectors_as_bytes() {
        let parsed = parse_diskstats(" 259 0 nvme0n1 10 0 20 0 30 0 40 0 0 0 0 0 0 0\n");
        assert_eq!(parsed.get("nvme0n1"), Some(&(20 * 512, 40 * 512)));
        assert!(parse_diskstats("malformed").is_empty());
    }
}
