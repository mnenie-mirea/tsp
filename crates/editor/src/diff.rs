use crate::edit_log::EditOp;

pub fn diff(old: &str, new: &str) -> Vec<EditOp> {
    if old == new {
        return Vec::new();
    }
    if old.is_empty() {
        return vec![EditOp::Insert {
            at: 0,
            text: new.to_string(),
        }];
    }
    if new.is_empty() {
        return vec![EditOp::Delete {
            at: 0,
            text: old.to_string(),
        }];
    }

    let mut pre = 0usize;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        pre += a.len_utf8();
    }

    let mut suf = 0usize;
    for (a, b) in old[pre..].chars().rev().zip(new[pre..].chars().rev()) {
        if a != b {
            break;
        }
        suf += a.len_utf8();
    }

    let d_start = pre;
    let d_end = old.len() - suf;
    let i_end = new.len() - suf;

    let mut ops = Vec::with_capacity(2);
    if d_end > d_start {
        ops.push(EditOp::Delete {
            at: d_start,
            text: old[d_start..d_end].to_string(),
        });
    }
    if i_end > d_start {
        ops.push(EditOp::Insert {
            at: d_start,
            text: new[d_start..i_end].to_string(),
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(s: &str, ops: &[EditOp]) -> String {
        let mut out = s.to_string();
        for op in ops {
            crate::edit_log::apply_to_string(&mut out, op);
        }
        out
    }

    fn assert_roundtrip(old: &str, new: &str) {
        let ops = diff(old, new);
        assert_eq!(apply(old, &ops), new, "diff({old:?} -> {new:?})");
    }

    #[test]
    fn typing_at_end() {
        let ops = diff("hello", "hello  world");
        assert_eq!(
            ops,
            vec![EditOp::Insert {
                at: 5,
                text: "  world".into()
            }]
        );
    }

    #[test]
    fn delete_middle() {
        let ops = diff("abcdef", "abef");
        assert_eq!(
            ops,
            vec![EditOp::Delete {
                at: 2,
                text: "cd".into()
            }]
        );
    }

    #[test]
    fn replace_selection() {
        let ops = diff("abXcd", "abYYcd");
        assert_eq!(
            ops,
            vec![
                EditOp::Delete {
                    at: 2,
                    text: "X".into()
                },
                EditOp::Insert {
                    at: 2,
                    text: "YY".into()
                },
            ]
        );
    }

    #[test]
    fn unicode_boundaries() {
        assert_roundtrip("héllo", "héeéllo");
        assert_roundtrip("привет мир", "привет, мир!");
        assert_roundtrip("日本語だ", "日本語です");
        assert_roundtrip("😀😁", "😀😂😁");
    }

    #[test]
    fn empty_sides() {
        assert_roundtrip("", "abc");
        assert_roundtrip("abc", "");
        assert_roundtrip("", "");
    }

    #[test]
    fn identical_input() {
        assert!(diff("same", "same").is_empty());
    }

    #[test]
    fn whole_text_replacement() {
        assert_roundtrip("aaaaaaaaaaa", "bbbb");
        assert_roundtrip("aa", "a");
    }
}
