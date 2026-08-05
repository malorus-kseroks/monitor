use clap::{Parser, Subcommand, ValueEnum};
use std::{path::PathBuf, str::FromStr, time::Duration};

use crate::i18n::Language;

#[derive(Debug, Parser)]
#[command(
    name = "kernox-monitor",
    version,
    about = "Capability-aware system and workstation monitor"
)]
pub struct Cli {
    /// Fast telemetry interval (250ms..60s)
    #[arg(short, long, value_parser = parse_interval)]
    pub interval: Option<Duration>,

    /// UI language: auto, en, uk, de, fr, es
    #[arg(short = 'l', long, value_parser = parse_language)]
    pub lang: Option<Language>,

    #[arg(short = 'p', long, value_enum)]
    pub page: Option<Page>,

    #[arg(long, value_enum)]
    pub color: Option<ColorMode>,

    #[arg(long)]
    pub ascii: bool,

    #[arg(short = 'c', long, conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub no_config: bool,

    /// Docker/Podman endpoint; may be specified more than once
    #[arg(long = "engine")]
    pub engines: Vec<String>,

    #[arg(long)]
    pub service_provider: Option<String>,

    #[arg(long)]
    pub show_unsupported: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print capability and provider diagnostics, then exit
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    Overview,
    Processes,
    Storage,
    Containers,
    Network,
    Services,
    Hardware,
}

impl Page {
    pub const ALL: [Self; 7] = [
        Self::Overview,
        Self::Processes,
        Self::Storage,
        Self::Containers,
        Self::Network,
        Self::Services,
        Self::Hardware,
    ];
    pub const fn linux_only(self) -> bool {
        matches!(self, Self::Services | Self::Hardware)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

fn parse_language(value: &str) -> Result<Language, String> {
    Language::from_str(value)
}

pub fn parse_interval(value: &str) -> Result<Duration, String> {
    let normalized = value.trim().to_ascii_lowercase();
    let seconds = if let Some(ms) = normalized.strip_suffix("ms") {
        ms.trim()
            .parse::<f64>()
            .map_err(|_| format!("invalid duration: {value}"))?
            / 1000.0
    } else if let Some(seconds) = normalized.strip_suffix('s') {
        seconds
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("invalid duration: {value}"))?
    } else {
        normalized
            .parse::<f64>()
            .map_err(|_| format!("invalid duration: {value}"))?
    };
    if !seconds.is_finite() || !(0.25..=60.0).contains(&seconds) {
        return Err("interval must be finite and between 250ms and 60s".into());
    }
    Ok(Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn interval_bounds() {
        assert!(parse_interval("250ms").is_ok());
        assert!(parse_interval("60s").is_ok());
        for bad in ["0", "-1", "NaN", "inf", "61s", "249ms"] {
            assert!(parse_interval(bad).is_err(), "{bad}");
        }
    }

    proptest! {
        #[test]
        fn millisecond_intervals_round_trip(milliseconds in 250_u64..=60_000) {
            let parsed = parse_interval(&format!("{milliseconds}ms")).expect("valid interval");
            prop_assert_eq!(parsed, Duration::from_millis(milliseconds));
        }
    }
}
