use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    chart::AreaChart,
    h_flex,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, InteractiveElement as _, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div, px,
};

use monitor::collector::Snapshot;
use monitor::config::MonitorConfig;

const MAX_POINTS: usize = 600;
const ACCENT: gpui_kit::Hsla = gpui_kit::hsla(210.0 / 360.0, 0.65, 0.58, 1.0);
const PURPLE: gpui_kit::Hsla = gpui_kit::hsla(270.0 / 360.0, 0.55, 0.65, 1.0);
const GREEN: gpui_kit::Hsla = gpui_kit::hsla(110.0 / 360.0, 0.5, 0.6, 1.0);
const YELLOW: gpui_kit::Hsla = gpui_kit::hsla(45.0 / 360.0, 0.8, 0.6, 1.0);

pub struct MonitorView {
    history: Vec<Snapshot>,
    read_offset: u64,
    log_path: PathBuf,
    config: MonitorConfig,
    paused: bool,
    last_error: Option<String>,
    _tail_task: Option<()>,
    _subscriptions: Vec<Subscription>,
}

impl MonitorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let log_path = MonitorConfig::log_path();
        let config = monitor::config::load_config(&MonitorConfig::config_path());

        let mut view = Self {
            history: Vec::new(),
            read_offset: 0,
            log_path,
            config,
            paused: false,
            last_error: None,
            _tail_task: None,
            _subscriptions: Vec::new(),
        };
        view.read_history_file();

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(1))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if !view.paused {
                            view.tail_file();
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        view
    }

    fn read_history_file(&mut self) {
        match std::fs::read(&self.log_path) {
            Ok(bytes) => {
                let mut history = Vec::new();
                for line in bytes.split(|&b| b == b'\n') {
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(snap) = serde_json::from_slice::<Snapshot>(line) {
                        history.push(snap);
                    }
                }
                self.read_offset = bytes.len() as u64;
                if history.len() > MAX_POINTS {
                    history.drain(0..history.len() - MAX_POINTS);
                }
                self.history = history;
            }
            Err(e) => self.last_error = Some(format!("нет файла лога: {e}")),
        }
    }

    fn tail_file(&mut self) {
        let Ok(meta) = std::fs::metadata(&self.log_path) else {
            return;
        };
        let size = meta.len();
        if size < self.read_offset {
            self.read_offset = 0;
            self.history.clear();
        }
        if size == self.read_offset {
            self.last_error = None;
            return;
        }
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut f) = std::fs::File::open(&self.log_path) else {
            return;
        };
        if f.seek(SeekFrom::Start(self.read_offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return;
        }
        self.read_offset = size;
        let mut partial: Vec<u8> = Vec::new();
        for line in buf.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            match serde_json::from_slice::<Snapshot>(line) {
                Ok(snap) => self.history.push(snap),
                Err(_) => partial = line.to_vec(),
            }
        }
        if !partial.is_empty() {
            self.read_offset -= (partial.len() + 1) as u64;
        }
        if self.history.len() > MAX_POINTS {
            let drop = self.history.len() - MAX_POINTS;
            self.history.drain(0..drop);
        }
        self.last_error = None;
    }

    fn latest(&self) -> Option<&Snapshot> {
        self.history.last()
    }

    fn render_stat_card(
        &self,
        icon: IconName,
        label: &str,
        value: String,
        color: gpui_kit::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.35))
            .min_w(px(200.))
            .child(
                div()
                    .text_color(color)
                    .child(Icon::new(icon).with_size(gpui_kit::component::Size::Medium)),
            )
            .child(
                v_flex()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(label.to_string())),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(value),
                    ),
            )
            .into_any_element_type()
    }

    fn render_chart_card(
        &self,
        title: &str,
        chart: impl gpui_kit::IntoElement,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        v_flex()
            .flex_1()
            .min_w(px(380.))
            .h(px(260.))
            .p_3()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(title.to_string())),
            )
            .child(div().flex_1().min_h_0().child(chart))
            .into_any_element_type()
    }
}

trait IntoAnyElementExt: Sized {
    fn into_any_element_type(self) -> gpui_kit::AnyElement;
}

impl<E: gpui_kit::IntoElement> IntoAnyElementExt for E {
    fn into_any_element_type(self) -> gpui_kit::AnyElement {
        self.into_any_element()
    }
}

impl Render for MonitorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        let latest = self.latest().cloned();
        let t = &self.config.thresholds;
        let latest2 = latest.clone();
        let latest3 = latest.clone();
        let latest4 = latest.clone();
        let history = self.history.clone();

        let cpu_chart = AreaChart::new(history.clone())
            .id("cpu-chart")
            .x(|s: &Snapshot| SharedString::from(fmt_time(s.timestamp)))
            .y(|s: &Snapshot| s.cpu_percent as f64)
            .stroke(ACCENT)
            .fill(ACCENT.opacity(0.15))
            .name("CPU %")
            .y_domain(0.0, 100.0)
            .reference_line(t.cpu_percent as f64)
            .grid(true);

        let mem_chart = AreaChart::new(history.clone())
            .id("mem-chart")
            .x(|s: &Snapshot| SharedString::from(fmt_time(s.timestamp)))
            .y(|s: &Snapshot| s.memory_percent as f64)
            .stroke(PURPLE)
            .fill(PURPLE.opacity(0.15))
            .name("Память %")
            .y_domain(0.0, 100.0)
            .reference_line(t.memory_percent as f64)
            .grid(true);

        let net_chart = AreaChart::new(history)
            .id("net-chart")
            .x(|s: &Snapshot| SharedString::from(fmt_time(s.timestamp)))
            .y(|s: &Snapshot| s.net_rx_kbps as f64)
            .stroke(GREEN)
            .fill(GREEN.opacity(0.15))
            .name("RX КБ/с")
            .y(|s: &Snapshot| s.net_tx_kbps as f64)
            .stroke(YELLOW)
            .name("TX КБ/с")
            .grid(true);

        let uptime = latest.as_ref().map(|s| {
            SharedString::from(format!(
                "последняя точка: {}",
                fmt_time(s.timestamp)
            ))
        });

        v_flex()
            .id("monitor-root")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .child(Icon::new(IconName::Cpu).text_color(ACCENT))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child("Editor Monitor"),
                        )
                        .child(div().flex_1())
                        .when_some(uptime, |el, u| {
                            el.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(u),
                            )
                        })
                        .child(
                            Button::new("pause")
                                .icon(if self.paused {
                                    IconName::Play
                                } else {
                                    IconName::Pause
                                })
                                .small()
                                .ghost()
                                .tooltip(if self.paused { "Продолжить" } else { "Пауза" })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.paused = !this.paused;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("refresh")
                                .icon(IconName::RotateCw)
                                .small()
                                .ghost()
                                .tooltip("Перечитать лог")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.read_history_file();
                                    cx.notify();
                                })),
                        ),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    
                    .p_4()
                    .child(
                        v_flex()
                            .gap_4()
                            .child(
                                h_flex().gap_3().flex_wrap().children([
                                    match latest {
                                        Some(s) => self.render_stat_card(
                                            IconName::Cpu,
                                            "CPU",
                                            format!("{:.1}%", s.cpu_percent),
                                            if s.cpu_percent >= t.cpu_percent { YELLOW } else { ACCENT },
                                            cx,
                                        ),
                                        None => placeholder_card("нет данных", cx),
                                    },
                                    match latest2 {
                                        Some(s) => self.render_stat_card(
                                            IconName::MemoryStick,
                                            "Память",
                                            format!(
                                                "{:.1}% · {}/{} МБ",
                                                s.memory_percent, s.memory_used_mb, s.memory_total_mb
                                            ),
                                            if s.memory_percent >= t.memory_percent { YELLOW } else { PURPLE },
                                            cx,
                                        ),
                                        None => placeholder_card("нет данных", cx),
                                    },
                                    match latest3 {
                                        Some(s) => self.render_stat_card(
                                            IconName::HardDrive,
                                            "Диск",
                                            format!(
                                                "{:.1}% · своб. {:.0} ГБ",
                                                s.disk_used_percent, s.disk_free_gb
                                            ),
                                            if s.disk_used_percent >= t.disk_percent { YELLOW } else { GREEN },
                                            cx,
                                        ),
                                        None => placeholder_card("нет данных", cx),
                                    },
                                    match latest4 {
                                        Some(s) => self.render_stat_card(
                                            IconName::Network,
                                            "Сеть ↓/↑",
                                            format!("{:.0} / {:.0} КБ/с", s.net_rx_kbps, s.net_tx_kbps),
                                            ACCENT,
                                            cx,
                                        ),
                                        None => placeholder_card("нет данных", cx),
                                    },
                                ]),
                            )
                            .child(
                                h_flex()
                                    .gap_4()
                                    .flex_wrap()
                                    .children([self.render_chart_card("CPU %", cpu_chart, cx)]),
                            )
                            .child(
                                h_flex()
                                    .gap_4()
                                    .flex_wrap()
                                    .children([self.render_chart_card("Память %", mem_chart, cx)]),
                            )
                            .child(
                                h_flex()
                                    .gap_4()
                                    .flex_wrap()
                                    .children([self.render_chart_card("Сеть КБ/с", net_chart, cx)]),
                            )
                            .when_some(self.last_error.clone(), |el, err| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().warning)
                                        .child(SharedString::from(err)),
                                )
                            }),
                    ),
            )
    }
}

fn placeholder_card(text: &str, cx: &mut Context<MonitorView>) -> gpui_kit::AnyElement {
    div()
        .px_4()
        .py_3()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .min_w(px(200.))
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(text.to_string()))
        .into_any_element_type()
}

fn fmt_time(ts: u64) -> String {
    let secs = ts % 60;
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

fn main() {
    let options = gpui_kit::WindowOptions {
        titlebar: Some(gpui_kit::component::TitleBar::title_bar_options()),
        window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds {
            origin: gpui_kit::Point::default(),
            size: gpui_kit::Size {
                width: px(980.),
                height: px(720.),
            },
        })),
        ..Default::default()
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.activate(true);
            gpui_kit::open_window(options, cx, |_, cx| cx.new(MonitorView::new))
                .expect("failed to open window");
        });
}
