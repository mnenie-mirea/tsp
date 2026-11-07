use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Editor, EditorState, Input, InputEvent, InputState},
    separator::Separator,
    status_bar::StatusBar,
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, WeakEntity, Window, actions, div, px,
};

use crate::{AppConfig, Document, Theme as ConfigTheme};

actions!(
    editor,
    [
        NewDoc,
        OpenFiles,
        Save,
        SaveAs,
        CloseTab,
        PrintDoc,
        FindOpen,
        Quit,
        ToggleSettings,
        CloseFind,
    ]
);

pub struct EditorApp {
    docs: Vec<Document>,
    active: usize,
    next_id: u64,

    editors: HashMap<u64, Entity<EditorState>>,
    find_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,

    config: AppConfig,
    config_path: PathBuf,

    cursors: HashMap<u64, usize>,

    find_open: bool,
    case_sensitive: bool,
    matches: Vec<Range<usize>>,
    match_idx: Option<usize>,

    last_autosave: Instant,
    show_settings: bool,
    modal: Option<Modal>,
    status: String,
}

enum Modal {
    CloseDoc(usize),
    Quit,
}

impl EditorApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_path = dirs::config_dir()
            .map(|d| d.join("editor").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"));
        let config = load_config(&config_path);
        apply_theme(&config, cx);

        let find_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(SharedString::from("поиск")));
        let replace_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(SharedString::from("замена")));

        let mut docs = Vec::new();
        let mut editors = HashMap::new();
        let mut subscriptions = Vec::new();
        docs.push(Document::empty(0));
        let (first_editor, sub) = Self::make_editor(0, "", window, cx);
        editors.insert(0, first_editor);
        subscriptions.push(sub);

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                if this
                    .update(cx, |s, cx| {
                        s.autosave();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Self {
            docs,
            active: 0,
            next_id: 1,
            editors,
            find_input,
            replace_input,
            _subscriptions: subscriptions,
            config,
            config_path,
            cursors: HashMap::new(),
            find_open: false,
            case_sensitive: false,
            matches: Vec::new(),
            match_idx: None,
            last_autosave: Instant::now(),
            show_settings: false,
            modal: None,
            status: "Готово".into(),
        }
    }

    fn make_editor(
        doc_id: u64,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<EditorState>, Subscription) {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(true)
                .default_value(text.to_string())
        });
        let sub = cx.subscribe_in(&editor, window, move |this: &mut Self, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_doc_text(doc_id, cx);
            }
        });
        (editor, sub)
    }

    fn make_editor_for(
        &mut self,
        doc_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        let text = self
            .docs
            .iter()
            .find(|d| d.id == doc_id)
            .map(|d| d.text.clone())
            .unwrap_or_default();
        let (editor, sub) = Self::make_editor(doc_id, &text, window, cx);
        self.editors.insert(doc_id, editor.clone());
        self._subscriptions.push(sub);
        editor
    }

    fn sync_doc_text(&mut self, doc_id: u64, cx: &mut Context<Self>) {
        let Some(editor) = self.editors.get(&doc_id) else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        if let Some(doc) = self.docs.iter_mut().find(|d| d.id == doc_id) {
            doc.text = text;
            doc.sync();
        }
        if doc_id == self.docs[self.active].id {
            self.recompute_matches(cx);
        }
        cx.notify();
    }

    fn active_editor(&self) -> Option<&Entity<EditorState>> {
        self.editors.get(&self.docs[self.active].id)
    }

    fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        self.docs.push(Document::empty(id));
        self.active = self.docs.len() - 1;
        self.cursors.insert(id, 0);
        self.make_editor_for(id, window, cx);
        self.find_open = false;
        self.recompute_matches(cx);
        self.status = "Новый документ".into();
        self.focus_editor(window, cx);
        cx.notify();
    }

    fn open_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                let files = rfd::AsyncFileDialog::new()
                    .add_filter(
                        "Текст",
                        &[
                            "txt", "md", "rs", "toml", "json", "yaml", "yml", "csv", "log", "ini",
                            "conf", "html", "css", "js", "ts", "py", "sh", "c", "h", "cpp", "hpp",
                            "java", "go", "xml", "tex",
                        ],
                    )
                    .add_filter("Все файлы", &["*"])
                    .pick_files()
                    .await;
                let Some(files) = files else { return };
                let paths: Vec<PathBuf> =
                    files.into_iter().map(|f| f.path().to_owned()).collect();
                this.update_in(cx, |this, window, cx| {
                    this.open_paths(paths, window, cx);
                })
                .ok();
            })
            .detach();
    }

    fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        for f in paths {
            let id = self.next_id;
            self.next_id += 1;
            match Document::open(id, f) {
                Ok(doc) => {
                    self.docs.push(doc);
                    self.active = self.docs.len() - 1;
                    self.cursors.insert(id, 0);
                    self.make_editor_for(id, window, cx);
                }
                Err(e) => {
                    self.status = format!("Не удалось открыть: {e}");
                }
            }
        }
        self.recompute_matches(cx);
        self.focus_editor(window, cx);
        cx.notify();
    }

    fn save_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.docs[self.active].path.is_none() {
            self.save_current_as(window, cx);
            return;
        }
        match self.docs[self.active].save() {
            Ok(()) => self.status = format!("Сохранено: {}", self.docs[self.active].title),
            Err(e) => self.status = format!("Ошибка сохранения: {e}"),
        }
    }

    fn save_current_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_name = format!("{}.txt", self.docs[self.active].title);
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                let path = rfd::AsyncFileDialog::new()
                    .add_filter("Текст", &["txt", "md"])
                    .add_filter("Все файлы", &["*"])
                    .set_file_name(&default_name)
                    .save_file()
                    .await
                    .map(|f| f.path().to_owned());
                let Some(path) = path else { return };
                this.update_in(cx, |this, _, cx| {
                    match this.docs[this.active].save_to(&path) {
                        Ok(()) => this.status = format!("Сохранено как: {}", path.display()),
                        Err(e) => this.status = format!("Ошибка сохранения: {e}"),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
    }

    fn close_tab(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.docs.len() <= 1 {
            let doc_id = self.docs[0].id;
            self.docs[0] = Document::empty(doc_id);
            if let Some(editor) = self.editors.get(&doc_id).cloned() {
                editor.update(cx, |state, cx| state.set_value("", window, cx));
            }
            self.recompute_matches(cx);
            self.status = "Новый документ".into();
            return;
        }
        let doc_id = self.docs[idx].id;
        self.docs.remove(idx);
        self.editors.remove(&doc_id);
        if self.active >= self.docs.len() {
            self.active = self.docs.len() - 1;
        } else if idx < self.active {
            self.active -= 1;
        }
    }

    fn request_close(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.docs[idx].dirty {
            self.modal = Some(Modal::CloseDoc(idx));
        } else {
            self.close_tab(idx, window, cx);
        }
        cx.notify();
    }

    fn focus_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor().cloned() {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn apply_history(&mut self, window: &mut Window, cx: &mut Context<Self>, redo: bool) {
        let doc_id = self.docs[self.active].id;
        let (text, cursor_byte) = {
            let doc = &mut self.docs[self.active];
            let op = if redo { doc.redo() } else { doc.undo() };
            let Some(op) = op else { return };
            (doc.text.clone(), doc.cursor_after(&op))
        };
        self.cursors.insert(doc_id, cursor_byte);
        if let Some(editor) = self.editors.get(&doc_id) {
            editor.update(cx, |state, cx| {
                state.set_value(text, window, cx);
                state.set_selected_range(cursor_byte..cursor_byte, cx);
            });
        }
        self.recompute_matches(cx);
        cx.notify();
    }

    fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_history(window, cx, false);
    }

    fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_history(window, cx, true);
    }

    fn recompute_matches(&mut self, cx: &mut Context<Self>) {
        let doc = &self.docs[self.active];
        let find_text = self.find_input.read(cx).value().to_string();
        self.matches = if !self.find_open || find_text.is_empty() {
            Vec::new()
        } else {
            doc.find_all(&find_text, self.case_sensitive)
        };
        self.match_idx = match self.match_idx {
            Some(i) if i < self.matches.len() => Some(i),
            _ => None,
        };
    }

    fn jump_to_match(&mut self, window: &mut Window, cx: &mut Context<Self>, idx: usize) {
        if self.matches.is_empty() {
            return;
        }
        self.match_idx = Some(idx % self.matches.len());
        let range = self.matches[self.match_idx.unwrap()].clone();
        let doc_id = self.docs[self.active].id;
        if let Some(editor) = self.editors.get(&doc_id) {
            editor.update(cx, |state, cx| {
                state.set_selected_range(range, cx);
                state.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn next_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let next = self.match_idx.map(|i| i + 1).unwrap_or(0);
        self.jump_to_match(window, cx, next);
    }

    fn prev_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let prev = self
            .match_idx
            .map(|i| {
                if i == 0 {
                    self.matches.len() - 1
                } else {
                    i - 1
                }
            })
            .unwrap_or_else(|| self.matches.len() - 1);
        self.jump_to_match(window, cx, prev);
    }

    fn replace_current_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(idx) = self.match_idx else { return };
        let Some(range) = self.matches.get(idx).cloned() else {
            return;
        };
        let repl = self.replace_input.read(cx).value().to_string();
        let doc_id = self.docs[self.active].id;
        self.docs[self.active].replace_current(range, &repl);
        let text = self.docs[self.active].text.clone();
        if let Some(editor) = self.editors.get(&doc_id) {
            editor.update(cx, |state, cx| {
                state.set_value(text, window, cx);
            });
        }
        self.match_idx = None;
        self.recompute_matches(cx);
        self.status = "Заменено (1)".into();
        cx.notify();
    }

    fn replace_all_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let find_text = self.find_input.read(cx).value().to_string();
        if find_text.is_empty() {
            return;
        }
        let repl = self.replace_input.read(cx).value().to_string();
        let doc_id = self.docs[self.active].id;
        let needle = find_text.clone();
        let n = self.docs[self.active].replace_all(&needle, &repl);
        let text = self.docs[self.active].text.clone();
        if let Some(editor) = self.editors.get(&doc_id) {
            editor.update(cx, |state, cx| {
                state.set_value(text, window, cx);
            });
        }
        self.match_idx = None;
        self.recompute_matches(cx);
        self.status = if n > 0 {
            format!("Заменено вхождений: {n}")
        } else {
            "Совпадений не найдено".into()
        };
        cx.notify();
    }

    fn toggle_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = !self.find_open;
        if self.find_open {
            let find_input = self.find_input.clone();
            find_input.update(cx, |state, cx| state.focus(window, cx));
        }
        self.recompute_matches(cx);
        cx.notify();
    }

    fn close_find(&mut self, cx: &mut Context<Self>) {
        self.find_open = false;
        self.recompute_matches(cx);
        cx.notify();
    }

    fn autosave(&mut self) {
        if !self.config.autosave_enabled {
            return;
        }
        if self.last_autosave.elapsed() < self.config.autosave_interval() {
            return;
        }
        let mut saved = 0usize;
        for doc in self.docs.iter_mut() {
            if doc.dirty && doc.path.is_some() && doc.save().is_ok() {
                saved += 1;
            }
        }
        self.last_autosave = Instant::now();
        if saved > 0 {
            self.status = format!("Автосохранение: {saved} файл(ов)");
        }
    }

    fn print_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.docs[self.active].text.clone();
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                let path = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name("document.pdf")
                    .save_file()
                    .await
                    .map(|f| f.path().to_owned());
                let Some(path) = path else { return };
                this.update_in(cx, |this, _, cx| {
                    match crate::print::export_pdf(&text, &path) {
                        Ok(()) => this.status = format!("PDF сохранён: {}", path.display()),
                        Err(e) => this.status = format!("Печать: {e}"),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        save_config(&self.config_path, &self.config);
        self.show_settings = false;
        self.status = "Настройки сохранены".into();
        cx.notify();
    }

    fn select_tab(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if idx < self.docs.len() && idx != self.active {
            self.active = idx;
            self.recompute_matches(cx);
            self.focus_editor(window, cx);
            cx.notify();
        }
    }

    fn quit(&mut self, cx: &mut Context<Self>) {
        let any_dirty = self.docs.iter().any(|d| d.dirty);
        if any_dirty {
            self.modal = Some(Modal::Quit);
            cx.notify();
        } else {
            cx.quit();
        }
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = &self.docs[self.active];
        let dirty_mark = if doc.dirty { " •" } else { "" };
        let title: SharedString = format!("{}{}", doc.title, dirty_mark).into();
        let can_undo = doc.can_undo();
        let can_redo = doc.can_redo();

        h_flex()
            .items_center()
            .gap_1()
            .px_2()
            .child(
                Button::new("tb-new")
                    .icon(IconName::File)
                    .small()
                    .ghost()
                    .tooltip("Новый документ (⌘N)")
                    .on_click(cx.listener(|this, _, window, cx| this.new_doc(window, cx))),
            )
            .child(
                Button::new("tb-open")
                    .icon(IconName::FolderOpen)
                    .small()
                    .ghost()
                    .tooltip("Открыть… (⌘O)")
                    .on_click(cx.listener(|this, _, window, cx| this.open_files(window, cx))),
            )
            .child(
                Button::new("tb-save")
                    .icon(IconName::HardDrive)
                    .small()
                    .ghost()
                    .tooltip("Сохранить (⌘S)")
                    .on_click(cx.listener(|this, _, window, cx| this.save_current(window, cx))),
            )
            .child(
                Button::new("tb-save-as")
                    .icon(IconName::FileText)
                    .small()
                    .ghost()
                    .tooltip("Сохранить как… (⇧⌘S)")
                    .on_click(cx.listener(|this, _, window, cx| this.save_current_as(window, cx))),
            )
            .child(Separator::vertical())
            .child(
                Button::new("tb-undo")
                    .icon(IconName::Undo2)
                    .small()
                    .ghost()
                    .disabled(!can_undo)
                    .tooltip("Отменить (⌘Z)")
                    .on_click(cx.listener(|this, _, window, cx| this.undo(window, cx))),
            )
            .child(
                Button::new("tb-redo")
                    .icon(IconName::Redo2)
                    .small()
                    .ghost()
                    .disabled(!can_redo)
                    .tooltip("Повторить (⇧⌘Z)")
                    .on_click(cx.listener(|this, _, window, cx| this.redo(window, cx))),
            )
            .child(Separator::vertical())
            .child(
                Button::new("tb-find")
                    .icon(IconName::Search)
                    .small()
                    .ghost()
                    .tooltip("Найти… (⌘F)")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_find(window, cx))),
            )
            .child(
                Button::new("tb-print")
                    .icon(IconName::FileText)
                    .small()
                    .ghost()
                    .tooltip("Печать в PDF (⌘P)")
                    .on_click(cx.listener(|this, _, window, cx| this.print_doc(window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(title),
            )
            .child(
                Button::new("tb-theme")
                    .icon(match self.config.theme {
                        ConfigTheme::Dark => IconName::Sun,
                        ConfigTheme::Light => IconName::Moon,
                    })
                    .small()
                    .ghost()
                    .tooltip(match self.config.theme {
                        ConfigTheme::Dark => "Светлая тема",
                        ConfigTheme::Light => "Тёмная тема",
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.theme = match this.config.theme {
                            ConfigTheme::Dark => ConfigTheme::Light,
                            ConfigTheme::Light => ConfigTheme::Dark,
                        };
                        apply_theme(&this.config, cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("tb-settings")
                    .icon(IconName::Settings)
                    .small()
                    .ghost()
                    .tooltip("Настройки (⌘,)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_settings = !this.show_settings;
                        cx.notify();
                    })),
            )
    }

    fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (cur, n) = (
            self.match_idx.map(|i| i + 1).unwrap_or(0),
            self.matches.len(),
        );
        h_flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.35))
            .text_color(cx.theme().muted_foreground)
            .child("Найти:")
            .child(
                div()
                    .w(px(220.))
                    .text_color(cx.theme().foreground)
                    .child(Input::new(&self.find_input).small().cleanable(true)),
            )
            .child(SharedString::from(format!("{cur}/{n}")))
            .child(
                Button::new("find-prev")
                    .icon(IconName::ArrowUp)
                    .xsmall()
                    .ghost()
                    .tooltip("Предыдущее (⇧Enter)")
                    .on_click(cx.listener(|this, _, window, cx| this.prev_match(window, cx))),
            )
            .child(
                Button::new("find-next")
                    .icon(IconName::ArrowDown)
                    .xsmall()
                    .ghost()
                    .tooltip("Следующее (Enter)")
                    .on_click(cx.listener(|this, _, window, cx| this.next_match(window, cx))),
            )
            .child(
                Button::new("find-case")
                    .icon(IconName::CaseSensitive)
                    .xsmall()
                    .when(self.case_sensitive, |b| b.secondary())
                    .when(!self.case_sensitive, |b| b.ghost())
                    .tooltip("С учётом регистра")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.case_sensitive = !this.case_sensitive;
                        this.match_idx = None;
                        this.recompute_matches(cx);
                        cx.notify();
                    })),
            )
            .child(Separator::vertical())
            .child("Заменить:")
            .child(
                div()
                    .w(px(180.))
                    .text_color(cx.theme().foreground)
                    .child(Input::new(&self.replace_input).small().cleanable(true)),
            )
            .child(
                Button::new("replace-one")
                    .label("Заменить")
                    .xsmall()
                    .outline()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.replace_current_match(window, cx)),
                    ),
            )
            .child(
                Button::new("replace-all")
                    .label("Всё")
                    .xsmall()
                    .outline()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.replace_all_matches(window, cx)),
                    ),
            )
            .child(div().flex_1())
            .child(
                Button::new("find-close")
                    .icon(IconName::Close)
                    .xsmall()
                    .ghost()
                    .tooltip("Закрыть (Esc)")
                    .on_click(cx.listener(|this, _, _, cx| this.close_find(cx))),
            )
    }

    fn render_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs = h_flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border);
        for (i, doc) in self.docs.iter().enumerate() {
            let is_active = i == self.active;
            let label: SharedString = if doc.dirty {
                format!("● {}", doc.title).into()
            } else {
                doc.title.clone().into()
            };
            tabs = tabs.child(
                div()
                    .id(doc.id)
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .cursor_pointer()
                    .when(is_active, |el| {
                        el.bg(cx.theme().secondary_active)
                            .text_color(cx.theme().foreground)
                    })
                    .when(!is_active, |el| {
                        el.text_color(cx.theme().muted_foreground)
                            .hover(|s| s.bg(cx.theme().secondary))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_tab(i, window, cx);
                    }))
                    .child(label)
                    .child(
                        div()
                            .id(("close", doc.id))
                            .rounded_sm()
                            .p(px(1.))
                            .hover(|s| s.bg(cx.theme().danger.opacity(0.25)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_close(i, window, cx);
                            }))
                            .child(Icon::new(IconName::Close).xsmall()),
                    ),
            );
        }
        tabs = tabs.child(
            div()
                .id("add-tab")
                .cursor_pointer()
                .p_1()
                .rounded_sm()
                .text_color(cx.theme().muted_foreground)
                .hover(|s| s.bg(cx.theme().secondary))
                .on_click(cx.listener(|this, _, window, cx| this.new_doc(window, cx)))
                .child(Icon::new(IconName::Plus).small()),
        );
        tabs
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = &self.docs[self.active];
        let (line, col) = self
            .active_editor()
            .map(|e| {
                let pos = e.read(cx).cursor_position();
                (pos.line as usize + 1, pos.character as usize + 1)
            })
            .unwrap_or((1, 1));
        let path_text = match &doc.path {
            Some(p) => p.display().to_string(),
            None => "не сохранён".into(),
        };

        StatusBar::new().child(
            h_flex()
                .gap_2()
                .text_sm()
                .child(SharedString::from(format!("строка {line}, столбец {col}")))
                .child(Separator::vertical())
                .child(if doc.dirty {
                    div().text_color(cx.theme().warning).child("● изменён")
                } else {
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child("сохранён")
                })
                .child(Separator::vertical())
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(path_text),
                )
                .child(Separator::vertical())
                .child(self.status.clone()),
        )
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(420.))
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .child("Настройки"),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(SharedString::from(format!(
                        "Размер шрифта: {}",
                        self.config.font_size
                    )))
                    .child(
                        Button::new("font-minus")
                            .icon(IconName::Minus)
                            .xsmall()
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.font_size = (this.config.font_size - 1.).max(8.);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("font-plus")
                            .icon(IconName::Plus)
                            .xsmall()
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.font_size = (this.config.font_size + 1.).min(42.);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child("Тема:")
                    .child(
                        Button::new("set-dark")
                            .label("Тёмная")
                            .xsmall()
                            .when(self.config.theme == ConfigTheme::Dark, |b| b.secondary())
                            .when(self.config.theme != ConfigTheme::Dark, |b| b.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.theme = ConfigTheme::Dark;
                                apply_theme(&this.config, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("set-light")
                            .label("Светлая")
                            .xsmall()
                            .when(self.config.theme == ConfigTheme::Light, |b| b.secondary())
                            .when(self.config.theme != ConfigTheme::Light, |b| b.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.theme = ConfigTheme::Light;
                                apply_theme(&this.config, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("autosave")
                            .checked(self.config.autosave_enabled)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.config.autosave_enabled = *checked;
                                cx.notify();
                            })),
                    )
                    .child("Автосохранение"),
            )
            .when(self.config.autosave_enabled, |el| {
                el.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(SharedString::from(format!(
                            "Интервал: {} с",
                            self.config.autosave_interval_secs
                        )))
                        .child(
                            Button::new("interval-minus")
                                .icon(IconName::Minus)
                                .xsmall()
                                .outline()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.config.autosave_interval_secs =
                                        this.config.autosave_interval_secs.saturating_sub(5).max(5);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("interval-plus")
                                .icon(IconName::Plus)
                                .xsmall()
                                .outline()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.config.autosave_interval_secs =
                                        (this.config.autosave_interval_secs + 5).min(600);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("settings-cancel")
                            .label("Отмена")
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_settings = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("settings-save")
                            .label("Сохранить")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
    }

    fn render_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(modal) = &self.modal else {
            return div().into_any_element();
        };
        let content: AnyElementAlias = match modal {
            Modal::CloseDoc(idx) => {
                let idx = *idx;
                let title = self
                    .docs
                    .get(idx)
                    .map(|d| d.title.clone())
                    .unwrap_or_default();
                v_flex()
                    .gap_3()
                    .child(SharedString::from(format!("«{title}» изменён. Сохранить?")))
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("dlg-cancel")
                                    .label("Отмена")
                                    .outline()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.modal = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("dlg-no")
                                    .label("Не сохранять")
                                    .outline()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.modal = None;
                                        this.close_tab(idx, window, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("dlg-save")
                                    .label("Сохранить")
                                    .primary()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.modal = None;
                                        this.docs[idx].save().ok();
                                        this.close_tab(idx, window, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .into_any_element()
            }
            Modal::Quit => v_flex()
                .gap_3()
                .child("Есть несохранённые документы. Выйти?")
                .child(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("quit-cancel")
                                .label("Отмена")
                                .outline()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.modal = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("quit-no")
                                .label("Выйти без сохранения")
                                .danger()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.modal = None;
                                    cx.quit();
                                })),
                        )
                        .child(
                            Button::new("quit-save")
                                .label("Сохранить все")
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.modal = None;
                                    for doc in this.docs.iter_mut() {
                                        doc.save().ok();
                                    }
                                    cx.quit();
                                })),
                        ),
                )
                .into_any_element(),
        };

        div()
            .absolute()
            .inset_0()
            .bg(cx.theme().foreground.opacity(0.4))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(420.))
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(content),
            )
            .into_any_element()
    }
}

type AnyElementAlias = gpui_kit::AnyElement;

impl Render for EditorApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_font = px(self.config.font_size);

        v_flex()
            .id("editor-root")
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &NewDoc, window, cx| this.new_doc(window, cx)))
            .on_action(cx.listener(|this, _: &OpenFiles, window, cx| this.open_files(window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save_current(window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save_current_as(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let idx = this.active;
                this.request_close(idx, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrintDoc, window, cx| this.print_doc(window, cx)))
            .on_action(cx.listener(|this, _: &FindOpen, window, cx| this.toggle_find(window, cx)))
            .on_action(cx.listener(|this, _: &CloseFind, _, cx| this.close_find(cx)))
            .on_action(cx.listener(|this, _: &ToggleSettings, _, cx| {
                this.show_settings = !this.show_settings;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Quit, _, cx| this.quit(cx)))
            .child(TitleBar::new().child(self.render_title_bar(cx)))
            .when(self.find_open, |el| el.child(self.render_find_bar(cx)))
            .child(self.render_tabs(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .pb_1()
                    .text_size(editor_font)
                    .when_some(self.active_editor().cloned(), |el, editor| {
                        el.child(Editor::new(&editor).size_full())
                    }),
            )
            .child(self.render_status_bar(cx))
            .when(self.show_settings, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(cx.theme().foreground.opacity(0.25))
                        .child(self.render_settings(cx)),
                )
            })
            .when(self.modal.is_some(), |el| el.child(self.render_modal(cx)))
    }
}

fn apply_theme(config: &AppConfig, cx: &mut App) {
    Theme::change(
        match config.theme {
            ConfigTheme::Dark => ThemeMode::Dark,
            ConfigTheme::Light => ThemeMode::Light,
        },
        None,
        cx,
    );
}

fn load_config(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(path: &Path, config: &AppConfig) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = toml::to_string_pretty(config) {
        let _ = std::fs::write(path, s);
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewDoc, None),
        KeyBinding::new("cmd-o", OpenFiles, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-p", PrintDoc, None),
        KeyBinding::new("cmd-f", FindOpen, None),
        KeyBinding::new("escape", CloseFind, None),
        KeyBinding::new("cmd-,", ToggleSettings, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("editor-test-cfg-{}-{name}", std::process::id()))
    }

    #[test]
    fn config_missing_file_yields_default() {
        assert_eq!(load_config(&tmp_path("missing.toml")), AppConfig::default());
    }

    #[test]
    fn config_broken_toml_yields_default() {
        let p = tmp_path("broken.toml");
        std::fs::write(&p, "not valid toml [[[").unwrap();
        assert_eq!(load_config(&p), AppConfig::default());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn config_roundtrips_through_file() {
        let p = tmp_path("roundtrip.toml");
        let cfg = AppConfig {
            font_size: 21.0,
            theme: ConfigTheme::Light,
            autosave_enabled: false,
            autosave_interval_secs: 120,
        };
        save_config(&p, &cfg);
        assert_eq!(load_config(&p), cfg);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn save_config_creates_parent_dirs() {
        let p = std::env::temp_dir()
            .join(format!("editor-test-cfg-dir-{}", std::process::id()))
            .join("nested")
            .join("config.toml");
        save_config(&p, &AppConfig::default());
        assert!(p.exists());
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn find_matches_respect_case_toggle() {
        let mut doc = Document::from_str(1, "Ab aB AB ab", "t");
        doc.mark_external();
        assert_eq!(doc.find_all("ab", true).len(), 1);
        assert_eq!(doc.find_all("ab", false).len(), 4);
    }

    #[test]
    fn find_matches_stay_in_bounds_on_multibyte_text() {
        let mut doc = Document::from_str(1, "привет мир, привет", "t");
        doc.mark_external();
        let ms = doc.find_all("привет", false);
        assert_eq!(ms.len(), 2);
        for m in ms {
            assert_eq!(&doc.text[m], "привет");
        }
    }
}
