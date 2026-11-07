# Практическое задание 1-4

Создание простого текстового редактора на Rust.

**Описание:** необходимо разработать текстовый редактор с базовыми функциями, такими как создание, редактирование, сохранение и открытие текстовых файлов. Дополнительно можно реализовать функции отмены и повтора действий, поиск и замену текста.

**Цели:** практика работы с файловой системой, вводом/выводом данных, и базовыми элементами построения GUI-приложений на Rust (gpui-kit).

## Реализуемые задачи

### 1. Интерфейс приложения с возможностью открытия нового окна для редактирования текста

Я разработал многооконный интерфейс: каждое окно открывается через системное меню, `Document` хранится в списке открытых вкладок.

```rust
// crates/editor/src/app.rs
pub struct Document {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub title: String,
    pub text: String,
    pub dirty: bool,
    // ...
}
```

### 2. Открытие и сохранение файлов через диалоговые окна

Реализовал открытие/сохранение через нативные диалоги `rfd`:

```rust
// crates/editor/src/document.rs
pub fn open(id: u64, path: PathBuf) -> io::Result<Self> {
    let text = std::fs::read_to_string(&path)?;
    // ...
}

pub fn save(&mut self) -> io::Result<()> {
    if let Some(path) = &self.path {
        std::fs::write(path, &self.text)?;
    }
    Ok(())
}
```

```rust
// выбор файла
let file = rfd::FileDialog::new()
    .add_filter("Text", &["txt", "md", "rs"])
    .pick_file();
```

### 3. Отмена и повтор последних действий (Undo/Redo)

Я реализовал журнал правок на основе операций вставки/удаления: при каждом изменении вычисляется минимальный diff и записывается обратимая операция, undo/redo восстанавливают текст и позицию курсора.

```rust
// crates/editor/src/edit_log.rs
pub fn undo(&mut self) -> Option<EditOp> {
    // ...
}
pub fn redo(&mut self) -> Option<EditOp> {
    // ...
}
pub fn can_undo(&self) -> bool { /* ... */ }
pub fn can_redo(&self) -> bool { /* ... */ }
```

```rust
// crates/editor/src/diff.rs — вычисление правки между версиями текста
pub fn diff(old: &str, new: &str) -> Vec<EditOp> {
    if old == new { return Vec::new(); }
    // ...
}
```

### 4. Поиск и замена текста

Добавил поиск (с учётом и без учёта регистра) и одиночную/глобальную замену:

```rust
// crates/editor/src/search.rs
pub fn find_all(text: &str, needle: &str) -> Vec<Range<usize>> { /* ... */ }
pub fn find_all_ci(text: &str, needle: &str) -> Vec<Range<usize>> { /* ... */ }
```

```rust
// crates/editor/src/document.rs
pub fn replace_current(&mut self, range: Range<usize>, repl: &str) { /* ... */ }
pub fn replace_all(&mut self, needle: &str, repl: &str) -> usize { /* ... */ }
```

### 5. Копирование, вырезание и вставка текста

Реализовал работу с системным буфером обмена для выделенного фрагмента текста.

### 6. Автоматическое сохранение файла через заданные промежутки времени

Добавил автосохранение с настраиваемым интервалом:

```rust
// crates/editor/src/config.rs
pub struct AppConfig {
    pub font_size: f32,
    pub theme: Theme,
    pub autosave_enabled: bool,
    pub autosave_interval_secs: u64,
}

impl AppConfig {
    pub fn autosave_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.autosave_interval_secs.max(1))
    }
}
```

### 7. Поддержка работы с несколькими файлами одновременно (многооконный интерфейс)

Я обеспечил одновременную работу с несколькими документами: у каждого свой журнал правок, путь к файлу и флаг несохранённых изменений.

### 8. Настройка шрифта и цвета текста (темы)

Реализовал настройки размера шрифта и светлой/тёмной темы с сохранением в TOML-конфиг:

```rust
// crates/editor/src/config.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}
```

### 9. Вывод на печать текущего документа

Я реализовал печать документа через генерацию PDF (встраивание системного шрифта) с последующей отправкой на печать:

```rust
// crates/editor/src/print.rs
pub fn export_pdf(text: &str, out: &Path) -> Result<(), String> {
    let mut doc = PdfDocument::new("Editor");
    // выбор системного шрифта, разбиение на страницы, генерация PDF
}
```
