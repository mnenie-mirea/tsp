use std::ops::Range;

pub fn find_all(text: &str, needle: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        out.push(start..end);
        from = end;
    }
    out
}

pub fn find_all_ci(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    if text.is_ascii() && needle.is_ascii() {
        return find_all_ascii_ci(text.as_bytes(), needle.as_bytes());
    }
    find_all_unicode_ci(text, needle)
}

fn find_all_ascii_ci(haystack: &[u8], needle: &[u8]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let n = needle.len();
    let mut i = 0usize;
    while i + n <= haystack.len() {
        if haystack[i..i + n].eq_ignore_ascii_case(needle) {
            out.push(i..i + n);
            i += n;
        } else {
            i += 1;
        }
    }
    out
}

fn find_all_unicode_ci(text: &str, needle: &str) -> Vec<Range<usize>> {
    let nchars = needle.chars().count();
    let needle_lower = needle.to_lowercase();
    let offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + nchars <= offsets.len() {
        let start = offsets[i];
        let end = offsets.get(i + nchars).copied().unwrap_or(text.len());
        if text[start..end]
            .chars()
            .flat_map(char::to_lowercase)
            .eq(needle_lower.chars())
        {
            out.push(start..end);
            i += nchars;
        } else {
            i += 1;
        }
    }
    out
}

pub fn find_from(text: &str, needle: &str, start_byte: usize) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    let from = start_byte.min(text.len());
    let rel = text[from..].find(needle)?;
    let start = from + rel;
    Some(start..start + needle.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_non_overlapping() {
        let r = find_all("abcabcba", "abc");
        assert_eq!(r, vec![0..3, 3..6]);
    }

    #[test]
    fn empty_needle_gives_nothing() {
        assert!(find_all("text", "").is_empty());
        assert!(find_all_ci("text", "").is_empty());
        assert!(find_from("text", "", 0).is_none());
    }

    #[test]
    fn unicode_matches() {
        let r = find_all("привет привет", "привет");
        assert_eq!(r, vec![0..12, 13..25]);
    }

    #[test]
    fn case_insensitive() {
        let r = find_all_ci("Hello HELlo HELLO", "hello");
        assert_eq!(r, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn case_insensitive_unicode() {
        let r = find_all_ci("Привет ПРИВЕТ", "привет");
        assert_eq!(r, vec![0..12, 13..25]);
    }
}
