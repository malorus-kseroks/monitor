use serde::Serialize;
use std::time::Duration;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::RuntimeConfig,
    domain::{
        ContainerSnapshot, HardwareSnapshot, ModuleState, ProbeReport, Provider, SystemSnapshot,
    },
    providers::{
        platform,
        system::{SystemAction, SystemProvider},
    },
    sanitize::redact_uri,
};

#[cfg(feature = "containers")]
use crate::providers::containers::{ContainerAction, ContainerProvider};

#[derive(Debug)]
pub enum RuntimeEvent {
    System(ModuleState<SystemSnapshot>),
    Containers(ModuleState<ContainerSnapshot>),
    Platform(ModuleState<HardwareSnapshot>),
    #[cfg(feature = "containers")]
    ContainerLogs {
        title: String,
        lines: Vec<String>,
    },
    ActionResult(Result<String, String>),
}

pub struct Supervisor {
    pub events: mpsc::Receiver<RuntimeEvent>,
    pub system_actions: mpsc::Sender<SystemAction>,
    #[cfg(feature = "containers")]
    pub container_actions: mpsc::Sender<ContainerAction>,
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl Supervisor {
    pub fn start(config: &RuntimeConfig) -> Self {
        let (event_tx, events) = mpsc::channel(32);
        let (system_actions, system_rx) = mpsc::channel(8);
        #[cfg(feature = "containers")]
        let (container_actions, container_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let mut tasks = vec![spawn_system(
            config.interval,
            system_rx,
            event_tx.clone(),
            cancel.clone(),
        )];
        tasks.push(spawn_platform(
            config.service_provider.clone(),
            event_tx.clone(),
            cancel.clone(),
        ));
        #[cfg(feature = "containers")]
        tasks.push(spawn_containers(
            config.container_endpoints.clone(),
            container_rx,
            event_tx,
            cancel.clone(),
        ));
        Self {
            events,
            system_actions,
            #[cfg(feature = "containers")]
            container_actions,
            cancel,
            tasks,
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}

fn spawn_system(
    refresh: Duration,
    mut actions: mpsc::Receiver<SystemAction>,
    events: mpsc::Sender<RuntimeEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut provider = SystemProvider::new();
        let mut ticker = interval(refresh);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    let state = match provider.snapshot().await { Ok(snapshot) => ModuleState::Ready { snapshot }, Err(error) => ModuleState::Error { summary: error.to_string(), details: None } };
                    if events.send(RuntimeEvent::System(state)).await.is_err() { break; }
                }
                Some(action) = actions.recv() => {
                    let result = provider.execute(action).await.map(|receipt| format!("{}: {}", receipt.target, receipt.summary)).map_err(|error| error.to_string());
                    if events.send(RuntimeEvent::ActionResult(result)).await.is_err() { break; }
                }
            }
        }
    })
}

fn spawn_platform(
    service_provider: String,
    events: mpsc::Sender<RuntimeEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    let snapshot = platform::snapshot(&service_provider).await;
                    if events.send(RuntimeEvent::Platform(ModuleState::Ready { snapshot })).await.is_err() { break; }
                }
            }
        }
    })
}

#[cfg(feature = "containers")]
fn spawn_containers(
    endpoints: Vec<String>,
    mut actions: mpsc::Receiver<ContainerAction>,
    events: mpsc::Sender<RuntimeEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut provider = ContainerProvider::new(endpoints);
        let mut ticker = interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    let state = match provider.snapshot().await {
                        Ok(snapshot) if snapshot.data.containers.is_empty() => ModuleState::Empty { message: "No containers".into() },
                        Ok(snapshot) => ModuleState::Ready { snapshot },
                        Err(crate::domain::ProviderError::Unavailable(reason)) => ModuleState::Unavailable { reason, remediation: Some("Start Docker Desktop, Docker Engine, or a Podman API service".into()) },
                        Err(error) => ModuleState::Error { summary: error.to_string(), details: None },
                    };
                    if events.send(RuntimeEvent::Containers(state)).await.is_err() { break; }
                }
                Some(action) = actions.recv() => {
                    let event = match action {
                        ContainerAction::Logs { engine, id, name } => {
                            match provider.logs(&engine, &id).await {
                                Ok(lines) => RuntimeEvent::ContainerLogs { title: name, lines },
                                Err(error) => RuntimeEvent::ActionResult(Err(error.to_string())),
                            }
                        }
                        action => {
                            let result = provider.execute(action).await.map(|receipt| format!("{}: {}", receipt.target, receipt.summary)).map_err(|error| error.to_string());
                            RuntimeEvent::ActionResult(result)
                        }
                    };
                    if events.send(event).await.is_err() { break; }
                }
            }
        }
    })
}

#[derive(Debug, Serialize)]
struct DoctorReport<'a> {
    package_version: &'a str,
    platform: &'a str,
    architecture: &'a str,
    config: DoctorConfig,
    system: DoctorSection,
    containers: DoctorSection,
    platform_probes: Vec<ProbeReport>,
    legacy_secret_files_present: bool,
}

#[derive(Debug, Serialize)]
struct DoctorConfig {
    language: String,
    interval_ms: u128,
    default_page: String,
    color: String,
    ascii: bool,
    show_unsupported_modules: bool,
    container_endpoints: Vec<String>,
    service_provider: String,
    config_source: Option<String>,
}

impl From<&RuntimeConfig> for DoctorConfig {
    fn from(config: &RuntimeConfig) -> Self {
        Self {
            language: format!("{:?}", config.language).to_ascii_lowercase(),
            interval_ms: config.interval.as_millis(),
            default_page: format!("{:?}", config.default_page).to_ascii_lowercase(),
            color: format!("{:?}", config.color).to_ascii_lowercase(),
            ascii: config.ascii,
            show_unsupported_modules: config.show_unsupported_modules,
            container_endpoints: config
                .container_endpoints
                .iter()
                .map(|endpoint| redact_uri(endpoint))
                .collect(),
            service_provider: config.service_provider.clone(),
            config_source: config
                .config_source
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct DoctorSection {
    state: &'static str,
    summary: String,
}

pub async fn run_doctor(config: &RuntimeConfig, json: bool) -> anyhow::Result<()> {
    let mut system_provider = SystemProvider::new();
    let system = match system_provider.snapshot().await {
        Ok(snapshot) => DoctorSection {
            state: "ready",
            summary: format!(
                "{} CPU threads, {} processes, {} filesystems, {} interfaces",
                snapshot.data.cpu.per_core_percent.len(),
                snapshot.data.processes.len(),
                snapshot.data.storage.len(),
                snapshot.data.networks.len()
            ),
        },
        Err(error) => DoctorSection {
            state: "error",
            summary: error.to_string(),
        },
    };
    #[cfg(feature = "containers")]
    let containers = {
        let mut provider = ContainerProvider::new(config.container_endpoints.clone());
        match provider.snapshot().await {
            Ok(snapshot) => DoctorSection {
                state: "ready",
                summary: format!(
                    "{} engines, {} containers",
                    snapshot.data.engines.len(),
                    snapshot.data.containers.len()
                ),
            },
            Err(error) => DoctorSection {
                state: "unavailable",
                summary: error.to_string(),
            },
        }
    };
    #[cfg(not(feature = "containers"))]
    let containers = DoctorSection {
        state: "unavailable",
        summary: "containers feature disabled".into(),
    };
    let platform_probes = platform::snapshot(&config.service_provider)
        .await
        .data
        .provider_reports;
    let report = DoctorReport {
        package_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        config: DoctorConfig::from(config),
        system,
        containers,
        platform_probes,
        legacy_secret_files_present: std::path::Path::new(".monitor_sudo").exists()
            || std::path::Path::new(".aether_sudo").exists(),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("KernOX Monitor {} doctor", report.package_version);
        println!("platform: {} {}", report.platform, report.architecture);
        println!(
            "config: {}",
            report
                .config
                .config_source
                .as_ref()
                .map_or_else(|| "defaults".into(), Clone::clone)
        );
        println!(
            "legacy secret files present: {}",
            report.legacy_secret_files_present
        );
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{ColorMode, Page},
        i18n::Language,
    };

    #[test]
    fn doctor_redacts_endpoint_credentials() {
        let runtime = RuntimeConfig {
            language: Language::English,
            interval: Duration::from_millis(500),
            default_page: Page::Overview,
            color: ColorMode::Auto,
            ascii: false,
            show_unsupported_modules: false,
            container_endpoints: vec!["tcp://alice:secret@localhost:2375".into()],
            service_provider: "auto".into(),
            config_source: None,
        };
        let serialized = serde_json::to_string(&DoctorConfig::from(&runtime)).expect("serialize");
        assert!(!serialized.contains("secret"));
        assert!(serialized.contains("[redacted]"));
    }
}
