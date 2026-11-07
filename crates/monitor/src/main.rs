use std::sync::{Arc, Mutex};

use monitor::collector::{unix_now, AppState, Collector};
use monitor::config::{load_config, MonitorConfig};

fn print_usage() {
    println!(
        "monitor — служба мониторинга системных ресурсов (macOS launchd)

USAGE:
  monitor run                 запуск в foreground (для launchd)
  monitor install             установить launchd-агент (автозапуск)
  monitor uninstall           удалить агент
  monitor enable              включить автозапуск
  monitor disable             отключить (процесс + автозапуск)
  monitor status              статус службы

Конфиг: {}
Лог:    {}
Веб:    http://127.0.0.1:<порт> (порт в конфиге)",
        MonitorConfig::config_path().display(),
        MonitorConfig::log_path().display(),
    );
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "run" => run(),
        "install" => done(monitor::launchd::install()),
        "uninstall" => done(monitor::launchd::uninstall()),
        "enable" => done(monitor::launchd::enable()),
        "disable" => done(monitor::launchd::disable()),
        "status" => println!("Служба: {}", monitor::launchd::status()),
        _ => print_usage(),
    }
}

fn done(r: anyhow::Result<()>) {
    match r {
        Ok(()) => println!("OK"),
        Err(e) => {
            eprintln!("Ошибка: {e}");
            std::process::exit(1);
        }
    }
}

fn run() {
    let state = Arc::new(AppState {
        running_since: Mutex::new(unix_now()),
        ..Default::default()
    });

    let cfg_path = MonitorConfig::config_path();
    let mut cfg = load_config(&cfg_path);
    let mut cfg_mtime = std::fs::metadata(&cfg_path).and_then(|m| m.modified()).ok();

    if cfg.notifications_enabled {
        monitor::alerter::notify("Monitor", "Служба мониторинга запущена");
    }

    let mut collector = Collector::new();
    let mut alerts = monitor::alerter::AlertState::default();

    loop {
        // hot-reload конфига
        let mtime = std::fs::metadata(&cfg_path).and_then(|m| m.modified()).ok();
        if mtime != cfg_mtime {
            cfg_mtime = mtime;
            cfg = load_config(&cfg_path);
        }

        let snap = collector.snapshot();
        if cfg.log_enabled {
            if let Err(e) =
                monitor::collector::append_log(&MonitorConfig::log_path(), &snap, cfg.log_max_bytes)
            {
                *state.last_error.lock().unwrap() = Some(format!("log: {e}"));
            }
        }

        for msg in monitor::alerter::check_thresholds(&mut alerts, &cfg, &snap) {
            if cfg.notifications_enabled {
                monitor::alerter::notify("Monitor: порог ресурсов", &msg);
            }
        }

        state.push(snap);

        let period = cfg.interval();
        std::thread::sleep(period);
    }
}
