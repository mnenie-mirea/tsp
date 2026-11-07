#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOp {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
    Group { ops: Vec<EditOp> },
}

impl EditOp {
    pub fn is_empty(&self) -> bool {
        match self {
            EditOp::Insert { text, .. } => text.is_empty(),
            EditOp::Delete { text, .. } => text.is_empty(),
            EditOp::Group { ops } => ops.is_empty(),
        }
    }
}

pub fn apply_to_string(s: &mut String, op: &EditOp) {
    match op {
        EditOp::Insert { at, text } => s.insert_str(*at, text),
        EditOp::Delete { at, text } => {
            s.replace_range(*at..*at + text.len(), "");
        }
        EditOp::Group { ops } => {
            for op in ops {
                apply_to_string(s, op);
            }
        }
    }
}

pub fn cursor_after_apply(op: &EditOp) -> usize {
    match op {
        EditOp::Insert { at, text } => at + text.len(),
        EditOp::Delete { at, .. } => *at,
        EditOp::Group { ops } => ops.last().map(cursor_after_apply).unwrap_or(0),
    }
}

fn inverse(op: &EditOp) -> EditOp {
    match op {
        EditOp::Insert { at, text } => EditOp::Delete {
            at: *at,
            text: text.clone(),
        },
        EditOp::Delete { at, text } => EditOp::Insert {
            at: *at,
            text: text.clone(),
        },
        EditOp::Group { ops } => EditOp::Group {
            ops: ops.iter().rev().map(inverse).collect(),
        },
    }
}

#[derive(Default, Debug, Clone)]
pub struct EditLog {
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
    group: Vec<EditOp>,
    in_group: bool,
}

impl EditLog {
    pub fn begin_group(&mut self) {
        debug_assert!(!self.in_group, "nested groups are not supported");
        self.in_group = true;
        self.group.clear();
    }

    pub fn end_group(&mut self) {
        debug_assert!(self.in_group, "end_group without begin_group");
        self.in_group = false;
        if self.group.is_empty() {
            return;
        }
        let op = if self.group.len() == 1 {
            self.group.pop().unwrap()
        } else {
            EditOp::Group {
                ops: std::mem::take(&mut self.group),
            }
        };
        self.push_plain(op);
    }

    pub fn record(&mut self, op: EditOp) {
        debug_assert!(!self.in_group, "use begin_group/end_group to batch ops");
        self.push_plain(op);
    }

    pub fn record_group(&mut self, ops: Vec<EditOp>) {
        debug_assert!(!self.in_group);
        if ops.is_empty() {
            return;
        }
        self.begin_group();
        self.group.extend(ops);
        self.end_group();
    }

    fn push_plain(&mut self, op: EditOp) {
        if op.is_empty() {
            return;
        }
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            if try_merge(last, &op) {
                return;
            }
        }
        self.undo.push(op);
    }

    pub fn undo(&mut self) -> Option<EditOp> {
        let op = self.undo.pop()?;
        self.redo.push(op.clone());
        Some(inverse(&op))
    }

    pub fn redo(&mut self) -> Option<EditOp> {
        let op = self.redo.pop()?;
        self.undo.push(op.clone());
        Some(op)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.group.clear();
        self.in_group = false;
    }
}

fn try_merge(prev: &mut EditOp, next: &EditOp) -> bool {
    match (prev, next) {
        (EditOp::Insert { at, text }, EditOp::Insert { at: b, text: t })
            if *at + text.len() == *b =>
        {
            text.push_str(t);
            true
        }
        (EditOp::Delete { at, text }, EditOp::Delete { at: b, text: t }) if *b + t.len() == *at => {
            text.insert_str(0, t);
            *at = *b;
            true
        }
        (EditOp::Delete { at, text }, EditOp::Delete { at: b, text: t }) if *at == *b => {
            text.push_str(t);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_merges_into_one_op() {
        let mut log = EditLog::default();
        log.record(EditOp::Insert {
            at: 0,
            text: "a".into(),
        });
        log.record(EditOp::Insert {
            at: 1,
            text: "b".into(),
        });
        log.record(EditOp::Insert {
            at: 2,
            text: "c".into(),
        });
        assert!(log.can_undo());
        assert_eq!(
            log.undo(),
            Some(EditOp::Delete {
                at: 0,
                text: "abc".into()
            })
        );
        assert!(!log.can_undo());
    }

    #[test]
    fn non_adjacent_inserts_stay_separate() {
        let mut log = EditLog::default();
        log.record(EditOp::Insert {
            at: 0,
            text: "a".into(),
        });
        log.record(EditOp::Insert {
            at: 3,
            text: "b".into(),
        });
        log.undo();
        log.undo();
        assert_eq!(
            log.redo(),
            Some(EditOp::Insert {
                at: 0,
                text: "a".into()
            })
        );
    }

    #[test]
    fn backspaces_merge() {
        let mut log = EditLog::default();
        log.record(EditOp::Delete {
            at: 2,
            text: "c".into(),
        });
        log.record(EditOp::Delete {
            at: 1,
            text: "b".into(),
        });
        log.record(EditOp::Delete {
            at: 0,
            text: "a".into(),
        });
        assert_eq!(
            log.undo(),
            Some(EditOp::Insert {
                at: 0,
                text: "abc".into()
            })
        );
    }

    #[test]
    fn forward_deletes_merge() {
        let mut log = EditLog::default();
        log.record(EditOp::Delete {
            at: 0,
            text: "a".into(),
        });
        log.record(EditOp::Delete {
            at: 0,
            text: "b".into(),
        });
        log.undo();
        assert_eq!(
            log.redo(),
            Some(EditOp::Delete {
                at: 0,
                text: "ab".into()
            })
        );
    }

    #[test]
    fn group_undoes_atomically() {
        let mut log = EditLog::default();
        log.record_group(vec![
            EditOp::Delete {
                at: 0,
                text: "old".into(),
            },
            EditOp::Insert {
                at: 0,
                text: "new".into(),
            },
        ]);

        let mut s = "old".to_string();
        let inverse = log.undo().unwrap();
        apply_to_string(&mut s, &inverse);
        assert_eq!(s, "old");

        let redo = log.redo().unwrap();
        apply_to_string(&mut s, &redo);
        assert_eq!(s, "new");
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let mut log = EditLog::default();
        log.record(EditOp::Insert {
            at: 0,
            text: "a".into(),
        });
        log.undo();
        assert!(log.can_redo());
        log.record(EditOp::Insert {
            at: 0,
            text: "b".into(),
        });
        assert!(!log.can_redo());
    }

    #[test]
    fn cursor_after_group_uses_last_op() {
        let g = EditOp::Group {
            ops: vec![
                EditOp::Delete {
                    at: 0,
                    text: "old".into(),
                },
                EditOp::Insert {
                    at: 0,
                    text: "new".into(),
                },
            ],
        };
        assert_eq!(cursor_after_apply(&g), 3);
    }
}
