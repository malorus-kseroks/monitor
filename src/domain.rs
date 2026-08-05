use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Debug};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Read,
    Control,
    Follow,
    Scan,
    Pair,
    PrivilegedControl,
}

pub type CapabilitySet = BTreeSet<Capability>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReport {
    pub provider: String,
    pub version: Option<String>,
    pub capabilities: CapabilitySet,
    pub state: ProbeState,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    Ready,
    Degraded,
    Unavailable,
    PermissionDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot<T> {
    pub data: T,
    pub collected_at: DateTime<Utc>,
}

impl<T> Snapshot<T> {
    pub fn now(data: T) -> Self {
        Self {
            data,
            collected_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModuleState<T> {
    #[default]
    Loading,
    Ready {
        snapshot: Snapshot<T>,
    },
    Empty {
        message: String,
    },
    Degraded {
        snapshot: Option<Snapshot<T>>,
        reason: String,
    },
    Unavailable {
        reason: String,
        remediation: Option<String>,
    },
    PermissionDenied {
        reason: String,
        remediation: Option<String>,
    },
    Error {
        summary: String,
        details: Option<String>,
    },
    Stale {
        snapshot: Snapshot<T>,
        reason: String,
    },
}

impl<T> ModuleState<T> {
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { snapshot } | Self::Stale { snapshot, .. } => Some(&snapshot.data),
            Self::Degraded {
                snapshot: Some(snapshot),
                ..
            } => Some(&snapshot.data),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider is unavailable: {0}")]
    Unavailable(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("operation timed out: {0}")]
    Timeout(String),
    #[error("command failed ({code:?}): {summary}")]
    Exit { code: Option<i32>, summary: String },
    #[error("failed to parse provider output: {0}")]
    Parse(String),
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
    #[error("I/O error: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReceipt {
    pub target: String,
    pub summary: String,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSignal {
    pub provider: String,
    pub target: String,
    pub kind: String,
    pub observed_at: DateTime<Utc>,
}

#[async_trait]
pub trait Provider: Send {
    type Data: Clone + Debug + Send + Sync + 'static;
    type Action: Send + Sync + 'static;

    async fn probe(&mut self) -> ProbeReport;
    async fn snapshot(&mut self) -> Result<Snapshot<Self::Data>, ProviderError>;
    async fn execute(&mut self, action: Self::Action) -> Result<ActionReceipt, ProviderError>;
    fn subscribe(&mut self) -> Option<tokio::sync::mpsc::Receiver<ProviderSignal>> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub total_percent: f32,
    pub per_core_percent: Vec<f32>,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub start_time: u64,
    pub name: String,
    pub command: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub id: String,
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
    pub read_per_second: Option<f64>,
    pub write_per_second: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub name: String,
    pub addresses: Vec<String>,
    pub received_per_second: f64,
    pub transmitted_per_second: f64,
    pub total_received: u64,
    pub total_transmitted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub host_name: String,
    pub os_name: String,
    pub kernel_version: String,
    pub uptime_seconds: u64,
    pub load_average: Option<[f64; 3]>,
    pub cpu: CpuSnapshot,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub processes: Vec<ProcessEntry>,
    pub storage: Vec<StorageEntry>,
    pub networks: Vec<NetworkEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerEntry {
    pub engine: String,
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub cpu_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerSnapshot {
    pub engines: Vec<ProbeReport>,
    pub containers: Vec<ContainerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    pub id: String,
    pub name: String,
    pub state: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareSnapshot {
    pub provider_reports: Vec<ProbeReport>,
    pub gpus: Vec<HardwareMetricGroup>,
    pub sensors: Vec<HardwareMetric>,
    pub batteries: Vec<HardwareMetricGroup>,
    pub displays: Vec<HardwareMetricGroup>,
    pub services: Vec<ServiceEntry>,
    pub log_preview: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareMetricGroup {
    pub id: String,
    pub label: String,
    pub metrics: Vec<HardwareMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareMetric {
    pub label: String,
    pub value: String,
    pub severity: MetricSeverity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricSeverity {
    Normal,
    Warning,
    Critical,
    Unknown,
}
