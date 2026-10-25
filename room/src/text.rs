//! Text helpers shared by the panels and the boards: wrapping by a measured width or by a
//! character count, and shortening with an ellipsis.

/// Wrap `text` into lines no longer than `max_chars`, by words; paragraphs on `\n` are kept.
pub fn wrap_chars(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if word.is_empty() && line.is_empty() {
                continue;
            }
            let candidate_len = if line.is_empty() {
                word.chars().count()
            } else {
                line.chars().count() + 1 + word.chars().count()
            };
            if candidate_len > max_chars && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            } else {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
        }
        out.push(line);
    }
    out
}

/// Wrap by a measuring function (pixels), by words; a word wider than the width stands alone.
pub fn wrap_measured<F: FnMut(&str) -> f64>(
    text: &str,
    max_width: f64,
    mut measure: F,
) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if word.is_empty() {
                continue;
            }
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate) > max_width && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// Shorten to `width` characters with an ellipsis, as the boards and the hand-off do.
pub fn ellipsize(text: &str, width: usize) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= width {
        return t;
    }
    let mut s: String = t.chars().take(width.saturating_sub(1)).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_by_words() {
        let lines = wrap_chars("Press a control: each issues one fixed Nibli query", 20);
        assert!(lines
            .iter()
            .all(|l| l.chars().count() <= 20 || !l.contains(' ')));
        assert_eq!(lines[0], "Press a control:");
    }
    #[test]
    fn keeps_paragraphs() {
        assert_eq!(wrap_chars("a\nb", 10), vec!["a", "b"]);
    }
    #[test]
    fn ellipsizes() {
        assert_eq!(ellipsize("abcdef", 4), "abc…");
        assert_eq!(ellipsize("ab  cd", 10), "ab cd");
    }
    #[test]
    fn measured_wrap_uses_the_measure() {
        let lines = wrap_measured("one two three", 7.0, |s| s.len() as f64);
        assert_eq!(lines, vec!["one two", "three"]);
    }
}
