use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, Networks, System};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: u64,
    pub cpu_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub memory_percent: f32,
    pub disk_used_percent: f32,
    pub disk_free_gb: f32,
    pub disk_total_gb: f32,
    pub net_rx_kbps: f32,
    pub net_tx_kbps: f32,
}

#[derive(Default)]
pub struct AppState {
    pub latest: Mutex<Option<Snapshot>>,
    pub history: Mutex<Vec<Snapshot>>,
    pub running_since: Mutex<u64>,
    pub last_error: Mutex<Option<String>>,
}

impl AppState {
    pub fn push(&self, snap: Snapshot) {
        *self.latest.lock().unwrap() = Some(snap.clone());
        let mut h = self.history.lock().unwrap();
        h.push(snap);
        let keep = 600;
        if h.len() > keep {
            let drop = h.len() - keep;
            h.drain(0..drop);
        }
    }
}

pub struct Collector {
    sys: System,
    disks: Disks,
    networks: Networks,
    last_rx: u64,
    last_tx: u64,
    had_prev: bool,
}

impl Collector {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let mut collector = Self {
            sys,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last_rx: 0,
            last_tx: 0,
            had_prev: false,
        };
        // Первый замер CPU требует интервала для осмысленного значения.
        std::thread::sleep(std::time::Duration::from_millis(200));
        collector.sys.refresh_cpu_usage();
        collector
    }

    pub fn snapshot(&mut self) -> Snapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        let cpu_percent = self.sys.global_cpu_usage();

        let mem_total = self.sys.total_memory() / 1024 / 1024;
        let mem_used = self.sys.used_memory() / 1024 / 1024;
        let memory_percent = if mem_total > 0 {
            mem_used as f32 * 100.0 / mem_total as f32
        } else {
            0.0
        };

        let (disk_total, disk_avail) = self
            .disks
            .list()
            .iter()
            .find(|d| d.mount_point() == Path::new("/"))
            .map(|d| (d.total_space(), d.available_space()))
            .unwrap_or((0, 0));
        let disk_used_percent = if disk_total > 0 {
            (disk_total - disk_avail) as f32 * 100.0 / disk_total as f32
        } else {
            0.0
        };

        let mut rx = 0u64;
        let mut tx = 0u64;
        for data in self.networks.values() {
            rx += data.total_received();
            tx += data.total_transmitted();
        }
        let (net_rx_kbps, net_tx_kbps) = if self.had_prev {
            let secs = 1.0; // refresh() использует внутренний интервал; считаем от периода сбора
            (
                (rx.saturating_sub(self.last_rx)) as f32 / 1024.0 / secs,
                (tx.saturating_sub(self.last_tx)) as f32 / 1024.0 / secs,
            )
        } else {
            (0.0, 0.0)
        };
        self.last_rx = rx;
        self.last_tx = tx;
        self.had_prev = true;

        Snapshot {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            cpu_percent,
            memory_used_mb: mem_used,
            memory_total_mb: mem_total,
            memory_percent,
            disk_used_percent,
            disk_free_gb: disk_avail as f32 / 1024.0 / 1024.0 / 1024.0,
            disk_total_gb: disk_total as f32 / 1024.0 / 1024.0 / 1024.0,
            net_rx_kbps,
            net_tx_kbps,
        }
    }
}

pub fn append_log(path: &Path, snap: &Snapshot, max_bytes: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.metadata().map(|m| m.len()).unwrap_or(0) > max_bytes {
        let _ = std::fs::rename(path, path.with_extension("jsonl.old"));
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(snap).unwrap())
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_roundtrip_and_rotation() {
        let p = std::env::temp_dir().join(format!("monitor-test-{}.jsonl", std::process::id()));
        let snap = Snapshot {
            timestamp: 1,
            cpu_percent: 12.5,
            memory_used_mb: 1,
            memory_total_mb: 2,
            memory_percent: 50.0,
            disk_used_percent: 10.0,
            disk_free_gb: 1.0,
            disk_total_gb: 10.0,
            net_rx_kbps: 0.0,
            net_tx_kbps: 0.0,
        };
        append_log(&p, &snap, 10_000).unwrap();
        append_log(&p, &snap, 10_000).unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert_eq!(content.lines().count(), 2);
        let parsed: Snapshot = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(parsed.cpu_percent, 12.5);

        append_log(&p, &snap, 10).unwrap();
        assert!(!p.exists() || std::fs::read_to_string(&p).unwrap().lines().count() <= 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn history_is_bounded() {
        let state = AppState::default();
        for i in 0..700 {
            state.push(Snapshot {
                timestamp: i,
                cpu_percent: 0.0,
                memory_used_mb: 0,
                memory_total_mb: 0,
                memory_percent: 0.0,
                disk_used_percent: 0.0,
                disk_free_gb: 0.0,
                disk_total_gb: 0.0,
                net_rx_kbps: 0.0,
                net_tx_kbps: 0.0,
            });
        }
        assert_eq!(state.history.lock().unwrap().len(), 600);
        assert_eq!(state.latest.lock().unwrap().as_ref().unwrap().timestamp, 699);
    }

    #[test]
    fn collector_produces_sane_snapshot() {
        let mut c = Collector::new();
        let s = c.snapshot();
        assert!(s.cpu_percent >= 0.0 && s.cpu_percent <= 100.0);
        assert!(s.memory_percent >= 0.0 && s.memory_percent <= 100.0);
        assert!(s.disk_total_gb > 0.0);
    }
}
