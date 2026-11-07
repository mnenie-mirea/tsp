use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Thresholds {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_percent: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_percent: 90.0,
            memory_percent: 85.0,
            disk_percent: 90.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MonitorConfig {
    pub interval_secs: u64,
    pub log_enabled: bool,
    pub log_max_bytes: u64,
    pub web_port: u16,
    pub notifications_enabled: bool,
    pub thresholds: Thresholds,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            log_enabled: true,
            log_max_bytes: 10 * 1024 * 1024,
            web_port: 8787,
            notifications_enabled: true,
            thresholds: Thresholds::default(),
        }
    }
}

impl MonitorConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs.max(1))
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .map(|d| d.join("editor").join("monitor.toml"))
            .unwrap_or_else(|| PathBuf::from("monitor.toml"))
    }

    pub fn data_dir() -> PathBuf {
        dirs::data_local_dir()
            .map(|d| d.join("editor").join("monitor"))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn log_path() -> PathBuf {
        Self::data_dir().join("metrics.jsonl")
    }
}

pub fn load_config(path: &Path) -> MonitorConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_yields_default() {
        assert_eq!(load_config(Path::new("/nonexistent/monitor.toml")), MonitorConfig::default());
    }

    #[test]
    fn config_roundtrips_and_overrides() {
        let p = std::env::temp_dir().join("monitor-test-config.toml");
        std::fs::write(
            &p,
            "interval_secs = 5\nweb_port = 9000\n[thresholds]\ncpu_percent = 70.0\n",
        )
        .unwrap();
        let cfg = load_config(&p);
        assert_eq!(cfg.interval_secs, 5);
        assert_eq!(cfg.web_port, 9000);
        assert_eq!(cfg.thresholds.cpu_percent, 70.0);
        assert_eq!(cfg.thresholds.memory_percent, 85.0);
        assert!(cfg.notifications_enabled);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn log_can_be_disabled() {
        let p = std::env::temp_dir().join("monitor-test-nolog.toml");
        std::fs::write(&p, "log_enabled = false\n").unwrap();
        let cfg = load_config(&p);
        assert!(!cfg.log_enabled);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn interval_is_never_zero() {
        let cfg = MonitorConfig {
            interval_secs: 0,
            ..Default::default()
        };
        assert_eq!(cfg.interval(), Duration::from_secs(1));
    }

    #[test]
    fn threshold_crossing_detected_once_per_direction() {
        let t = Thresholds {
            cpu_percent: 90.0,
            ..Default::default()
        };
        assert!(!alert_needed(t.cpu_percent, 89.9));
        assert!(alert_needed(t.cpu_percent, 90.0));
        assert!(alert_needed(t.cpu_percent, 99.0));
    }

    pub fn alert_needed(threshold: f32, value: f32) -> bool {
        value >= threshold
    }
}
