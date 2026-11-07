use notify_rust::Notification;

use crate::collector::Snapshot;
use crate::config::MonitorConfig;

#[derive(Default, Clone, Copy)]
pub struct AlertState {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
}

impl AlertState {
    fn exceeded(&mut self, key: bool, active: bool) -> Option<bool> {
        if active && !key {
            Some(true)
        } else if !active && key {
            Some(false)
        } else {
            None
        }
    }
}

/// Возвращает список сообщений о входе/выходе за пороги.
pub fn check_thresholds(
    state: &mut AlertState,
    cfg: &MonitorConfig,
    snap: &Snapshot,
) -> Vec<String> {
    let mut messages = Vec::new();
    let t = &cfg.thresholds;

    if let Some(on) = state.exceeded(state.cpu, snap.cpu_percent >= t.cpu_percent) {
        messages.push(if on {
            format!("⚠️ CPU: {:.0}% (порог {:.0}%)", snap.cpu_percent, t.cpu_percent)
        } else {
            "✅ CPU: вернулся в норму".into()
        });
        state.cpu = on;
    }
    if let Some(on) = state.exceeded(state.memory, snap.memory_percent >= t.memory_percent) {
        messages.push(if on {
            format!(
                "⚠️ Память: {:.0}% ({} из {} МБ)",
                snap.memory_percent, snap.memory_used_mb, snap.memory_total_mb
            )
        } else {
            "✅ Память: вернулась в норму".into()
        });
        state.memory = on;
    }
    if let Some(on) = state.exceeded(state.disk, snap.disk_used_percent >= t.disk_percent) {
        messages.push(if on {
            format!(
                "⚠️ Диск: занято {:.0}% (свободно {:.1} ГБ)",
                snap.disk_used_percent, snap.disk_free_gb
            )
        } else {
            "✅ Диск: вернулся в норму".into()
        });
        state.disk = on;
    }
    messages
}

pub fn notify(summary: &str, body: &str) {
    let _ = Notification::new()
        .summary(summary)
        .body(body)
        .appname("Monitor")
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Thresholds;

    fn snap(cpu: f32, mem: f32, disk: f32) -> Snapshot {
        Snapshot {
            timestamp: 0,
            cpu_percent: cpu,
            memory_used_mb: 0,
            memory_total_mb: 100,
            memory_percent: mem,
            disk_used_percent: disk,
            disk_free_gb: 1.0,
            disk_total_gb: 10.0,
            net_rx_kbps: 0.0,
            net_tx_kbps: 0.0,
        }
    }

    #[test]
    fn fires_once_and_recovers_once() {
        let cfg = MonitorConfig {
            thresholds: Thresholds {
                cpu_percent: 50.0,
                memory_percent: 50.0,
                disk_percent: 50.0,
            },
            ..Default::default()
        };
        let mut st = AlertState::default();

        assert_eq!(check_thresholds(&mut st, &cfg, &snap(10.0, 10.0, 10.0)).len(), 0);
        let alerts = check_thresholds(&mut st, &cfg, &snap(90.0, 10.0, 10.0));
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].contains("CPU"));
        assert_eq!(check_thresholds(&mut st, &cfg, &snap(95.0, 10.0, 10.0)).len(), 0);
        let alerts = check_thresholds(&mut st, &cfg, &snap(10.0, 10.0, 10.0));
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].contains("норму"));
        assert_eq!(check_thresholds(&mut st, &cfg, &snap(10.0, 10.0, 10.0)).len(), 0);
    }

    #[test]
    fn all_resources_reported() {
        let cfg = MonitorConfig::default();
        let mut st = AlertState::default();
        let alerts = check_thresholds(&mut st, &cfg, &snap(100.0, 100.0, 100.0));
        assert_eq!(alerts.len(), 3);
    }
}
