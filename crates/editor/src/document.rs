use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::diff;
use crate::edit_log::{EditLog, EditOp, apply_to_string, cursor_after_apply};
use crate::search;

#[derive(Debug)]
pub struct Document {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub text: String,
    pub log: EditLog,
    last_seen: String,
}

impl Document {
    pub fn empty(id: u64) -> Self {
        Self {
            id,
            title: "Untitled".into(),
            path: None,
            dirty: false,
            text: String::new(),
            log: EditLog::default(),
            last_seen: String::new(),
        }
    }

    pub fn from_str(id: u64, text: impl Into<String>, title: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id,
            title: title.into(),
            path: None,
            dirty: false,
            last_seen: text.clone(),
            text,
            log: EditLog::default(),
        }
    }

    pub fn open(id: u64, path: PathBuf) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?;
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Self {
            id,
            title,
            path: Some(path),
            dirty: false,
            last_seen: text.clone(),
            text,
            log: EditLog::default(),
        })
    }

    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "untitled document has no path")
        })?;
        self.save_to(&path)
    }

    pub fn save_to(&mut self, path: &Path) -> io::Result<()> {
        fs::write(path, &self.text)?;
        self.path = Some(path.to_path_buf());
        self.title = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.dirty = false;
        Ok(())
    }

    pub fn sync(&mut self) {
        if self.last_seen == self.text {
            return;
        }
        let ops = diff::diff(&self.last_seen, &self.text);
        self.last_seen.clone_from(&self.text);
        self.log.record_group(ops);
        self.dirty = true;
    }

    pub fn mark_external(&mut self) {
        self.last_seen.clone_from(&self.text);
    }

    pub fn undo(&mut self) -> Option<EditOp> {
        let inverse = self.log.undo()?;
        apply_to_string(&mut self.text, &inverse);
        self.mark_external();
        self.dirty = true;
        Some(inverse)
    }

    pub fn redo(&mut self) -> Option<EditOp> {
        let op = self.log.redo()?;
        apply_to_string(&mut self.text, &op);
        self.mark_external();
        self.dirty = true;
        Some(op)
    }

    pub fn cursor_after(&self, op: &EditOp) -> usize {
        cursor_after_apply(op)
    }

    pub fn can_undo(&self) -> bool {
        self.log.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.log.can_redo()
    }

    pub fn find_all(&self, needle: &str, case_sensitive: bool) -> Vec<Range<usize>> {
        if case_sensitive {
            search::find_all(&self.text, needle)
        } else {
            search::find_all_ci(&self.text, needle)
        }
    }

    pub fn replace_current(&mut self, range: Range<usize>, repl: &str) {
        let matched = self.text[range.clone()].to_string();
        let mut ops = Vec::with_capacity(2);
        ops.push(EditOp::Delete {
            at: range.start,
            text: matched,
        });
        if !repl.is_empty() {
            ops.push(EditOp::Insert {
                at: range.start,
                text: repl.to_string(),
            });
        }
        self.log.record_group(ops);
        self.text.replace_range(range, repl);
        self.mark_external();
        self.dirty = true;
    }

    pub fn replace_all(&mut self, needle: &str, repl: &str) -> usize {
        let matches = search::find_all(&self.text, needle);
        let count = matches.len();
        if count == 0 {
            return 0;
        }
        let mut ops = Vec::with_capacity(count * 2);
        for m in matches.iter().rev() {
            let matched = self.text[m.clone()].to_string();
            ops.push(EditOp::Delete {
                at: m.start,
                text: matched,
            });
            if !repl.is_empty() {
                ops.push(EditOp::Insert {
                    at: m.start,
                    text: repl.to_string(),
                });
            }
        }
        for op in &ops {
            apply_to_string(&mut self.text, op);
        }
        self.log.record_group(ops);
        self.mark_external();
        self.dirty = true;
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("editor-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn open_reads_and_tracks_path() {
        let p = temp_file("doc.md", "hello");
        let d = Document::open(1, p.clone()).unwrap();
        assert_eq!(d.text, "hello");
        assert_eq!(d.title, "doc.md");
        assert!(!d.dirty);
    }

    #[test]
    fn save_writes_and_clears_dirty() {
        let p = temp_file("doc.txt", "");
        let mut d = Document::from_str(1, "некоторый текст", "doc.txt");
        d.dirty = true;
        d.save_to(&p).unwrap();
        assert!(!d.dirty);
        assert_eq!(fs::read_to_string(&p).unwrap(), "некоторый текст");
    }

    #[test]
    fn sync_turns_widget_edits_into_ops() {
        let mut d = Document::from_str(1, "abc", "x");
        d.text.push_str("def");
        d.sync();
        assert!(d.dirty);
        assert_eq!(
            d.undo().unwrap(),
            EditOp::Delete {
                at: 3,
                text: "def".into()
            }
        );
        assert_eq!(d.text, "abc");
    }

    #[test]
    fn mark_external_before_programmatic_change_is_not_logged() {
        let mut d = Document::from_str(1, "abc", "x");
        d.text = "xyz".into();
        d.mark_external();
        d.sync();
        assert!(!d.can_undo());
        assert!(!d.dirty);
    }

    #[test]
    fn undo_redo_restores_text() {
        let mut d = Document::from_str(1, "", "x");
        d.text = "hello world".into();
        d.sync();
        d.undo();
        d.undo();
        assert_eq!(d.text, "");
        d.redo();
        d.redo();
        assert_eq!(d.text, "hello world");
    }

    #[test]
    fn replace_current_undoes_as_one_step() {
        let mut d = Document::from_str(1, "hello", "x");
        d.replace_current(0..5, "bye");
        assert_eq!(d.text, "bye");
        d.undo();
        assert_eq!(d.text, "hello");
    }

    #[test]
    fn replace_all_undoes_atomically() {
        let mut d = Document::from_str(1, "aa b aa c aa", "x");
        assert_eq!(d.replace_all("aa", "X"), 3);
        assert_eq!(d.text, "X b X c X");
        d.undo();
        assert_eq!(d.text, "aa b aa c aa");
    }
}

#[cfg(test)]
mod open_tests {
    use super::*;

    #[test]
    fn open_fails_on_non_utf8_file() {
        let p = std::env::temp_dir().join(format!("editor-test-bin-{}", std::process::id()));
        std::fs::write(&p, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        assert!(Document::open(1, p.clone()).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn open_fails_on_missing_file() {
        assert!(Document::open(1, PathBuf::from("/nonexistent/file.txt")).is_err());
    }

    #[test]
    fn dirty_persists_after_undo_until_saved() {
        let mut d = Document::from_str(1, "hello", "t");
        d.text.push_str(" world");
        d.sync();
        assert!(d.dirty);
        d.undo();
        assert_eq!(d.text, "hello");
        assert!(d.dirty, "undo не снимает dirty: нужно сохранение");
        d.save_to(&std::env::temp_dir().join(format!("editor-test-dirty-{}.txt", std::process::id())))
            .unwrap();
        assert!(!d.dirty);
    }
}
