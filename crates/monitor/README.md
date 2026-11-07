# Практическое задание 5-8

Создание службы мониторинга системных ресурсов.

**Описание:** разработка службы, которая собирает информацию о системных ресурсах (использование CPU, память, диск, сеть) и сохраняет данные в лог-файл для последующего анализа.

**Цели:** изучение разработки системных служб (агентов ОС), работа с системными API и управление службой через Rust.

> [!IMPORTANT]
> 🖥️ **GPU-интерфейс (нативная web-морда для просмотра метрик):**
> реализован в крейте [monitor-ui](../monitor-ui/README.md) — просмотрщик метрик Editor Monitor в реальном времени.

## Реализуемые задачи

### 1. Служба, автоматически запускаемая при старте системы

Я реализовал службу как launchd-агент (macOS): генерация plist-файла, установка и регистрация в системе:

```rust
// crates/monitor/src/launchd.rs
pub fn plist_content(binary: &str, log_out: &str, log_err: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
  <dict>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    ...
  </dict>
</plist>"#)
}

pub fn install() -> anyhow::Result<()> { /* ... */ }
pub fn uninstall() -> anyhow::Result<()> { /* ... */ }
pub fn enable()  -> anyhow::Result<()> { /* ... */ }
pub fn disable() -> anyhow::Result<()> { /* ... */ }
```

### 2. Сбор данных о процессоре, памяти, диске и сети

Реализовал сборщик метрик на базе `sysinfo`: снимок (snapshot) ресурсов формируется с заданной периодичностью:

```rust
// crates/monitor/src/collector.rs
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

pub struct Collector {
    sys: System,
    disks: Disks,
    networks: Networks,
    // ...
}
```

Скорость сети вычисляется как дельта счётчиков между опросами, история хранится в кольцевом буфере на 600 точек.

### 3. Запись данных в лог-файл с заданной периодичностью

Я добавил запись снапшотов в лог с настройкой интервала и ротацией по размеру:

```rust
// crates/monitor/src/config.rs
pub struct MonitorConfig {
    pub interval_secs: u64,
    pub log_enabled: bool,
    pub log_max_bytes: u64,
    pub web_port: u16,
    pub notifications_enabled: bool,
    pub thresholds: Thresholds,
}
```

### 4. Оповещения при превышении пороговых значений

Реализовал пороги для CPU/памяти/диска с уведомлениями при входе за порог и возврате в норму (без повторных спам-сообщений):

```rust
// crates/monitor/src/alerter.rs
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
    // ...
}
```

### 5. Конфигурирование службы через файл настроек

Я обеспечил конфигурацию через TOML-файл настроек (интервал опроса, пороги, логирование, порт web-интерфейса) с значениями по умолчанию.

### 6. Уведомления в реальном времени о состоянии службы

Добавил системные уведомления (`notify-rust`) о старте службы, срабатывании порогов и ошибках сбора метрик:

```rust
// crates/monitor/src/alerter.rs
use notify_rust::Notification;
// отправка уведомления пользователю
```

### 7. Остановка и перезапуск службы

Я реализовал управление жизненным циклом службы: команды `install` / `uninstall` / `enable` / `disable` / `status` для загрузки и выгрузки агента из системы.

### 8. Просмотр и анализ собранных данных

Для просмотра метрик в реальном времени я разработал GPU-интерфейс — крейт [monitor-ui](../monitor-ui/README.md): нативное приложение на gpui-kit, подключающееся к службе и отображающее графики CPU, памяти, диска и сети.
