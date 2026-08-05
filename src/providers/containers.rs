use bollard::{
    Docker,
    query_parameters::{
        ListContainersOptionsBuilder, LogsOptionsBuilder, RestartContainerOptionsBuilder,
        StatsOptionsBuilder, StopContainerOptionsBuilder,
    },
};
use futures_util::{StreamExt, stream::FuturesUnordered};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use crate::{
    domain::{
        ActionReceipt, Capability, CapabilitySet, ContainerEntry, ContainerSnapshot, ProbeReport,
        ProbeState, ProviderError, Snapshot,
    },
    sanitize::{redact_uri, terminal_text},
};

#[derive(Debug, Clone)]
pub enum ContainerAction {
    Refresh,
    Start {
        engine: String,
        id: String,
    },
    Stop {
        engine: String,
        id: String,
    },
    Restart {
        engine: String,
        id: String,
    },
    Logs {
        engine: String,
        id: String,
        name: String,
    },
}

struct EngineClient {
    id: String,
    endpoint: String,
    docker: Docker,
    kind: String,
}

pub struct ContainerProvider {
    engines: Vec<EngineClient>,
    configured_endpoints: Vec<String>,
}

impl ContainerProvider {
    pub fn new(configured_endpoints: Vec<String>) -> Self {
        Self {
            engines: vec![],
            configured_endpoints,
        }
    }

    async fn discover(&mut self) {
        if !self.engines.is_empty() {
            return;
        }
        let mut candidates: Vec<(String, Result<Docker, bollard::errors::Error>)> = self
            .configured_endpoints
            .iter()
            .map(|endpoint| (endpoint.clone(), Docker::connect_with_host(endpoint)))
            .collect();
        if candidates.is_empty() {
            candidates.push(("local".into(), Docker::connect_with_local_defaults()));
        }
        #[cfg(unix)]
        candidates.push(("podman".into(), Docker::connect_with_podman_defaults()));

        let mut seen = HashSet::new();
        for (endpoint, result) in candidates {
            if !seen.insert(endpoint.clone()) {
                continue;
            }
            let Ok(docker) = result else {
                continue;
            };
            if tokio::time::timeout(Duration::from_secs(2), docker.ping())
                .await
                .ok()
                .and_then(Result::ok)
                .is_none()
            {
                continue;
            }
            let version = tokio::time::timeout(Duration::from_secs(2), docker.version())
                .await
                .ok()
                .and_then(Result::ok);
            let kind = version
                .as_ref()
                .and_then(|item| item.platform.as_ref())
                .map(|platform| platform.name.clone())
                .unwrap_or_else(|| {
                    if endpoint.contains("podman") {
                        "Podman".into()
                    } else {
                        "Docker-compatible".into()
                    }
                });
            let id = stable_engine_id(&kind, &endpoint);
            self.engines.push(EngineClient {
                id,
                endpoint,
                docker,
                kind,
            });
        }
    }

    pub async fn snapshot(&mut self) -> Result<Snapshot<ContainerSnapshot>, ProviderError> {
        self.discover().await;
        if self.engines.is_empty() {
            return Err(ProviderError::Unavailable(
                "no local Docker/Podman API responded".into(),
            ));
        }
        let mut reports = Vec::new();
        let mut containers = Vec::new();
        for engine in &self.engines {
            let options = ListContainersOptionsBuilder::default().all(true).build();
            match tokio::time::timeout(
                Duration::from_secs(4),
                engine.docker.list_containers(Some(options)),
            )
            .await
            {
                Ok(Ok(items)) => {
                    reports.push(engine_report(engine, ProbeState::Ready, None));
                    let mut running_ids = Vec::new();
                    for item in items {
                        let id = item.id.unwrap_or_default();
                        let name = item
                            .names
                            .unwrap_or_default()
                            .into_iter()
                            .next()
                            .unwrap_or_default()
                            .trim_start_matches('/')
                            .to_owned();
                        let state = terminal_text(
                            &item
                                .state
                                .map_or_else(|| "unknown".into(), |state| format!("{state:?}")),
                            64,
                        );
                        if state.eq_ignore_ascii_case("running") && !id.is_empty() {
                            running_ids.push(id.clone());
                        }
                        containers.push(ContainerEntry {
                            engine: engine.id.clone(),
                            short_id: id.chars().take(12).collect(),
                            id,
                            name: terminal_text(&name, 256),
                            image: terminal_text(item.image.as_deref().unwrap_or("unknown"), 256),
                            state,
                            status: terminal_text(item.status.as_deref().unwrap_or("unknown"), 256),
                            cpu_percent: None,
                            memory_used: None,
                            memory_limit: None,
                        });
                    }
                    let stats = collect_engine_stats(engine, running_ids).await;
                    for container in containers
                        .iter_mut()
                        .filter(|container| container.engine == engine.id)
                    {
                        if let Some(sample) = stats.get(&container.id) {
                            container.cpu_percent = sample.cpu_percent;
                            container.memory_used = sample.memory_used;
                            container.memory_limit = sample.memory_limit;
                        }
                    }
                }
                Ok(Err(error)) => reports.push(engine_report(
                    engine,
                    ProbeState::Degraded,
                    Some(&terminal_text(&error.to_string(), 256)),
                )),
                Err(_) => reports.push(engine_report(
                    engine,
                    ProbeState::Degraded,
                    Some("list request timed out"),
                )),
            }
        }
        containers.sort_by(|a, b| a.engine.cmp(&b.engine).then(a.name.cmp(&b.name)));
        Ok(Snapshot::now(ContainerSnapshot {
            engines: reports,
            containers,
        }))
    }

    pub async fn execute(
        &mut self,
        action: ContainerAction,
    ) -> Result<ActionReceipt, ProviderError> {
        if matches!(action, ContainerAction::Refresh) {
            self.engines.clear();
            self.discover().await;
            return Ok(receipt("containers", "refreshed"));
        }
        let (engine_id, container_id) = match &action {
            ContainerAction::Start { engine, id }
            | ContainerAction::Stop { engine, id }
            | ContainerAction::Restart { engine, id }
            | ContainerAction::Logs { engine, id, .. } => (engine, id),
            ContainerAction::Refresh => unreachable!(),
        };
        let engine = self
            .engines
            .iter()
            .find(|item| &item.id == engine_id)
            .ok_or_else(|| ProviderError::Unavailable("container engine disappeared".into()))?;
        let result = match action {
            ContainerAction::Start { .. } => tokio::time::timeout(
                Duration::from_secs(10),
                engine.docker.start_container(container_id, None),
            )
            .await
            .map_err(|_| ProviderError::Timeout("container start".into()))?
            .map(|()| "started"),
            ContainerAction::Stop { .. } => {
                let options = StopContainerOptionsBuilder::default().t(10).build();
                tokio::time::timeout(
                    Duration::from_secs(15),
                    engine.docker.stop_container(container_id, Some(options)),
                )
                .await
                .map_err(|_| ProviderError::Timeout("container stop".into()))?
                .map(|()| "stopped")
            }
            ContainerAction::Restart { .. } => {
                let options = RestartContainerOptionsBuilder::default().t(10).build();
                tokio::time::timeout(
                    Duration::from_secs(20),
                    engine.docker.restart_container(container_id, Some(options)),
                )
                .await
                .map_err(|_| ProviderError::Timeout("container restart".into()))?
                .map(|()| "restarted")
            }
            ContainerAction::Refresh | ContainerAction::Logs { .. } => unreachable!(),
        };
        result
            .map(|summary| receipt(container_id, summary))
            .map_err(|error| ProviderError::Exit {
                code: None,
                summary: terminal_text(&error.to_string(), 512),
            })
    }

    pub async fn logs(
        &self,
        engine_id: &str,
        container_id: &str,
    ) -> Result<Vec<String>, ProviderError> {
        let engine = self
            .engines
            .iter()
            .find(|item| item.id == engine_id)
            .ok_or_else(|| ProviderError::Unavailable("container engine disappeared".into()))?;
        let options = LogsOptionsBuilder::default()
            .follow(false)
            .stdout(true)
            .stderr(true)
            .timestamps(true)
            .tail("500")
            .build();
        let mut stream = engine.docker.logs(container_id, Some(options));
        let read = async {
            let mut lines = Vec::new();
            let mut bytes = 0_usize;
            while let Some(item) = stream.next().await {
                let output = item.map_err(|error| ProviderError::Exit {
                    code: None,
                    summary: terminal_text(&error.to_string(), 512),
                })?;
                for line in String::from_utf8_lossy(output.as_ref()).lines() {
                    if lines.len() >= 500 || bytes >= 1024 * 1024 {
                        return Ok(lines);
                    }
                    let clean = terminal_text(line, 4096);
                    bytes = bytes.saturating_add(clean.len());
                    lines.push(clean);
                }
            }
            Ok(lines)
        };
        tokio::time::timeout(Duration::from_secs(5), read)
            .await
            .map_err(|_| ProviderError::Timeout("container logs".into()))?
    }
}

#[derive(Debug, Clone, Copy)]
struct ContainerStats {
    cpu_percent: Option<f64>,
    memory_used: Option<u64>,
    memory_limit: Option<u64>,
}

async fn collect_engine_stats(
    engine: &EngineClient,
    ids: Vec<String>,
) -> HashMap<String, ContainerStats> {
    let mut pending = FuturesUnordered::new();
    for id in ids.into_iter().take(16) {
        let docker = engine.docker.clone();
        pending.push(async move {
            let options = StatsOptionsBuilder::default()
                .stream(false)
                .one_shot(false)
                .build();
            let mut stream = docker.stats(&id, Some(options));
            let sample = tokio::time::timeout(Duration::from_secs(2), stream.next())
                .await
                .ok()
                .flatten()
                .and_then(Result::ok)?;
            let cpu = sample.cpu_stats.as_ref();
            let previous = sample.precpu_stats.as_ref();
            let cpu_percent = calculate_cpu_percent(
                cpu.and_then(|stats| stats.cpu_usage.as_ref())
                    .and_then(|usage| usage.total_usage),
                previous
                    .and_then(|stats| stats.cpu_usage.as_ref())
                    .and_then(|usage| usage.total_usage),
                cpu.and_then(|stats| stats.system_cpu_usage),
                previous.and_then(|stats| stats.system_cpu_usage),
                cpu.and_then(|stats| stats.online_cpus)
                    .or_else(|| {
                        cpu.and_then(|stats| stats.cpu_usage.as_ref())
                            .and_then(|usage| usage.percpu_usage.as_ref())
                            .map(|values| u32::try_from(values.len()).unwrap_or(u32::MAX))
                    })
                    .unwrap_or(1),
            );
            let memory_used = sample.memory_stats.as_ref().and_then(|stats| {
                stats
                    .usage
                    .or(stats.privateworkingset)
                    .or(stats.commitbytes)
            });
            let memory_limit = sample.memory_stats.as_ref().and_then(|stats| stats.limit);
            Some((
                id,
                ContainerStats {
                    cpu_percent,
                    memory_used,
                    memory_limit,
                },
            ))
        });
    }
    let mut result = HashMap::new();
    while let Some(sample) = pending.next().await {
        if let Some((id, stats)) = sample {
            result.insert(id, stats);
        }
    }
    result
}

fn calculate_cpu_percent(
    current: Option<u64>,
    previous: Option<u64>,
    system_current: Option<u64>,
    system_previous: Option<u64>,
    cpus: u32,
) -> Option<f64> {
    let cpu_delta = current?.saturating_sub(previous?);
    let system_delta = system_current?.saturating_sub(system_previous?);
    if system_delta == 0 {
        return None;
    }
    Some(cpu_delta as f64 / system_delta as f64 * f64::from(cpus) * 100.0)
}

fn engine_report(engine: &EngineClient, state: ProbeState, reason: Option<&str>) -> ProbeReport {
    ProbeReport {
        provider: format!("{} ({})", engine.kind, redact_uri(&engine.endpoint)),
        version: None,
        capabilities: [Capability::Read, Capability::Control, Capability::Follow]
            .into_iter()
            .collect::<CapabilitySet>(),
        state,
        reason: reason.map(str::to_owned),
    }
}

fn receipt(target: &str, summary: &str) -> ActionReceipt {
    ActionReceipt {
        target: target.into(),
        summary: summary.into(),
        completed_at: chrono::Utc::now(),
    }
}

fn stable_engine_id(kind: &str, endpoint: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in redact_uri(endpoint).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!(
        "{}-{hash:016x}",
        kind.to_ascii_lowercase().replace([' ', '/'], "-")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_cpu_delta_formula() {
        let value = calculate_cpu_percent(Some(150), Some(100), Some(1_200), Some(1_000), 4)
            .expect("valid sample");
        assert!((value - 100.0).abs() < f64::EPSILON);
        assert_eq!(
            calculate_cpu_percent(Some(1), Some(1), Some(5), Some(5), 1),
            None
        );
    }

    #[test]
    fn engine_identifier_is_deterministic_and_credential_independent() {
        let first = stable_engine_id("Docker", "tcp://alice:one@localhost:2375");
        let second = stable_engine_id("Docker", "tcp://bob:two@localhost:2375");
        assert_eq!(first, second);
        assert_ne!(first, stable_engine_id("Docker", "unix:///run/docker.sock"));
    }
}
