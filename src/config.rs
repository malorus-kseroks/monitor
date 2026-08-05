use crate::{
    cli::{Cli, ColorMode, Page},
    i18n::Language,
    sanitize::redact_uri,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid {source_name}: {message}")]
    Value {
        source_name: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub version: u32,
    pub language: Language,
    pub interval: String,
    pub default_page: Page,
    pub color: ColorMode,
    pub ascii: bool,
    pub show_unsupported_modules: bool,
    pub containers: ContainerConfig,
    pub services: ServiceConfig,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            version: 1,
            language: Language::Auto,
            interval: "500ms".into(),
            default_page: Page::Overview,
            color: ColorMode::Auto,
            ascii: false,
            show_unsupported_modules: false,
            containers: ContainerConfig::default(),
            services: ServiceConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContainerConfig {
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceConfig {
    pub provider: String,
}
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            provider: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfig {
    pub language: Language,
    #[serde(with = "duration_millis")]
    pub interval: Duration,
    pub default_page: Page,
    pub color: ColorMode,
    pub ascii: bool,
    pub show_unsupported_modules: bool,
    pub container_endpoints: Vec<String>,
    pub service_provider: String,
    pub config_source: Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn resolve(cli: &Cli) -> Result<Self, ConfigError> {
        let selected_path = cli.config.clone().or_else(default_config_path);
        let mut file = if cli.no_config {
            FileConfig::default()
        } else if let Some(path) = selected_path.as_deref().filter(|path| path.exists()) {
            load_file(path)?
        } else {
            FileConfig::default()
        };
        if file.version != 1 {
            return Err(ConfigError::Value {
                source_name: "config.version".into(),
                message: format!("expected 1, got {}", file.version),
            });
        }

        apply_env(&mut file)?;
        let file_interval =
            crate::cli::parse_interval(&file.interval).map_err(|message| ConfigError::Value {
                source_name: "config.interval".into(),
                message,
            })?;
        let interval = cli.interval.unwrap_or(file_interval);
        let language = cli.lang.unwrap_or(file.language).resolve();
        let default_page = cli.page.unwrap_or(file.default_page);
        let mut color = cli.color.unwrap_or(file.color);
        if color == ColorMode::Auto && env::var_os("NO_COLOR").is_some() {
            color = ColorMode::Never;
        }
        let ascii = cli.ascii || file.ascii;
        let show_unsupported_modules = cli.show_unsupported || file.show_unsupported_modules;
        let mut container_endpoints = if cli.engines.is_empty() {
            file.containers.endpoints
        } else {
            cli.engines.clone()
        };
        if container_endpoints.is_empty()
            && let Ok(host) = env::var("DOCKER_HOST")
        {
            container_endpoints.push(host);
        }
        validate_endpoints(&container_endpoints)?;
        let service_provider = cli
            .service_provider
            .clone()
            .unwrap_or(file.services.provider);
        if !matches!(
            service_provider.as_str(),
            "auto" | "systemd" | "openrc" | "runit" | "dinit" | "sysvinit"
        ) {
            return Err(ConfigError::Value {
                source_name: "service provider".into(),
                message: service_provider,
            });
        }

        Ok(Self {
            language,
            interval,
            default_page,
            color,
            ascii,
            show_unsupported_modules,
            container_endpoints,
            service_provider,
            config_source: if cli.no_config {
                None
            } else {
                selected_path.filter(|path| path.exists())
            },
        })
    }
}

fn load_file(path: &Path) -> Result<FileConfig, ConfigError> {
    let value = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&value).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn default_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|base| base.join("kernox-monitor").join("config.toml"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(base) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return Some(
                PathBuf::from(base)
                    .join("kernox-monitor")
                    .join("config.toml"),
            );
        }
        env::var_os("HOME").map(PathBuf::from).map(|home| {
            home.join(".config")
                .join("kernox-monitor")
                .join("config.toml")
        })
    }
}

fn apply_env(file: &mut FileConfig) -> Result<(), ConfigError> {
    if let Ok(value) = env::var("KERNOX_MONITOR_LANG") {
        file.language = Language::from_str(&value).map_err(|message| ConfigError::Value {
            source_name: "KERNOX_MONITOR_LANG".into(),
            message,
        })?;
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_INTERVAL") {
        file.interval = value;
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_DEFAULT_PAGE") {
        file.default_page = match value.to_ascii_lowercase().as_str() {
            "overview" => Page::Overview,
            "processes" => Page::Processes,
            "storage" => Page::Storage,
            "containers" => Page::Containers,
            "network" => Page::Network,
            "services" => Page::Services,
            "hardware" => Page::Hardware,
            _ => {
                return Err(ConfigError::Value {
                    source_name: "KERNOX_MONITOR_DEFAULT_PAGE".into(),
                    message: value,
                });
            }
        };
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_COLOR") {
        file.color = match value.to_ascii_lowercase().as_str() {
            "auto" => ColorMode::Auto,
            "always" => ColorMode::Always,
            "never" => ColorMode::Never,
            _ => {
                return Err(ConfigError::Value {
                    source_name: "KERNOX_MONITOR_COLOR".into(),
                    message: value,
                });
            }
        };
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_ASCII") {
        file.ascii = parse_bool("KERNOX_MONITOR_ASCII", &value)?;
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_SHOW_UNSUPPORTED") {
        file.show_unsupported_modules = parse_bool("KERNOX_MONITOR_SHOW_UNSUPPORTED", &value)?;
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_ENGINES") {
        file.containers.endpoints = value
            .split(',')
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .map(str::to_owned)
            .collect();
    }
    if let Ok(value) = env::var("KERNOX_MONITOR_SERVICE_PROVIDER") {
        file.services.provider = value;
    }
    Ok(())
}

fn parse_bool(name: &str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::Value {
            source_name: name.into(),
            message: value.into(),
        }),
    }
}

fn validate_endpoints(endpoints: &[String]) -> Result<(), ConfigError> {
    for endpoint in endpoints {
        let lower = endpoint.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("ssh://")
        {
            return Err(ConfigError::Value {
                source_name: "container endpoint".into(),
                message: format!(
                    "remote TLS/SSH connectors are not enabled in this build: {}",
                    redact_uri(endpoint)
                ),
            });
        }
        if lower.starts_with("tcp://") && !is_loopback_tcp(&lower) {
            return Err(ConfigError::Value {
                source_name: "container endpoint".into(),
                message: format!("plain remote TCP is forbidden: {}", redact_uri(endpoint)),
            });
        }
        if !lower.starts_with("unix://")
            && !lower.starts_with("npipe://")
            && !is_loopback_tcp(&lower)
        {
            return Err(ConfigError::Value {
                source_name: "container endpoint".into(),
                message: format!("unsupported connector: {}", redact_uri(endpoint)),
            });
        }
    }
    Ok(())
}

fn is_loopback_tcp(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("tcp://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    if let Some(bracketed) = host_port.strip_prefix('[') {
        return bracketed
            .split(']')
            .next()
            .is_some_and(|host| host == "::1");
    }
    host_port
        .split(':')
        .next()
        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1"))
}

mod duration_millis {
    use serde::Serializer;
    use std::time::Duration;
    pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u128(duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn strict_config_rejects_unknown_fields() {
        assert!(toml::from_str::<FileConfig>("version=1\nunknown=true").is_err());
    }
    #[test]
    fn rejects_remote_plain_tcp() {
        assert!(validate_endpoints(&["tcp://example.com:2375".into()]).is_err());
        assert!(validate_endpoints(&["tcp://localhost.evil:2375".into()]).is_err());
    }
    #[test]
    fn accepts_local_socket() {
        assert!(validate_endpoints(&["unix:///run/docker.sock".into()]).is_ok());
    }
    #[test]
    fn rejects_unimplemented_remote_connectors() {
        assert!(validate_endpoints(&["https://alice:secret@example.com".into()]).is_err());
        assert!(validate_endpoints(&["ssh://example.com".into()]).is_err());
    }

    #[test]
    fn explicit_default_interval_still_overrides_config() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        fs::write(&path, "version = 1\ninterval = \"1s\"\n").expect("write config");
        let cli = Cli::parse_from([
            "kernox-monitor",
            "--config",
            path.to_str().expect("UTF-8 path"),
            "--interval",
            "500ms",
            "doctor",
        ]);
        let runtime = RuntimeConfig::resolve(&cli).expect("resolve config");
        assert_eq!(runtime.interval, Duration::from_millis(500));
    }
}
