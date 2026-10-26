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

/// Wrap by measured pixels, breaking overlong tokens at UTF-8 character boundaries.
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
            if measure(&candidate) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            for ch in word.chars() {
                let mut next = line.clone();
                next.push(ch);
                if !line.is_empty() && measure(&next) > max_width {
                    out.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        out.push(line);
    }
    out
}

/// Wrap a proof without flattening its tree indentation. Continuations stay at the
/// same depth. Cap exceptionally deep indentation to leave room for the text.
pub fn wrap_preformatted<F: FnMut(&str) -> f64>(
    text: &str,
    max_width: f64,
    mut measure: F,
) -> Vec<String> {
    let mut out = Vec::new();
    for source in text.lines() {
        let mut indent: String = source.chars().take_while(|c| *c == ' ').collect();
        while !indent.is_empty() && measure(&indent) > max_width / 3.0 {
            indent.pop();
        }
        let width = (max_width - measure(&indent)).max(0.0);
        out.extend(
            wrap_measured(source.trim_start_matches(' '), width, &mut measure)
                .into_iter()
                .map(|line| format!("{indent}{line}")),
        );
    }
    out
}

/// A bounded view of a complete proof. Resizing may change the page count, so a
/// requested page is always clamped before navigation controls are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofPage {
    pub index: usize,
    pub count: usize,
    pub start: usize,
    pub end: usize,
    pub rows: usize,
}
impl ProofPage {
    pub fn new(total: usize, requested: usize, page_size: usize) -> Self {
        let size = page_size.max(1);
        let count = total.div_ceil(size).max(1);
        let index = requested.min(count - 1);
        let start = index * size;
        Self {
            index,
            count,
            start,
            end: start.saturating_add(size).min(total),
            rows: size.min(total),
        }
    }

    pub fn previous(self) -> Option<usize> {
        self.index.checked_sub(1)
    }

    pub fn next(self) -> Option<usize> {
        (self.index + 1 < self.count).then_some(self.index + 1)
    }
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

    #[test]
    fn long_proof_tokens_fit_without_losing_characters() {
        let token = format!("__abs_v1_{}", "a0150000".repeat(64));
        let text = format!("  ⊢ Derived: {token}\n    ▣ Asserted: exist(Lucy, Memory, Loaded).\n");
        let lines = wrap_preformatted(&text, 32.0, |s| s.chars().count() as f64);
        assert!(
            lines.len() > 14,
            "the screenshot's trace needs more than one page"
        );
        assert!(lines.iter().all(|line| line.chars().count() <= 32));
        assert!(lines.iter().all(|line| line.starts_with("  ")));
        assert!(lines.iter().any(|line| line.starts_with("    ▣")));
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(compact(&lines.join("\n")), compact(&text));
    }

    #[test]
    fn measured_wrap_handles_unicode_and_tiny_widths() {
        let lines = wrap_preformatted("  ⊢ αβγδεζ\n\n    ▣ 結論", 5.0, |s| {
            s.chars()
                .map(|c| if c == '結' || c == '論' { 2.0 } else { 1.0 })
                .sum()
        });
        assert!(
            lines.contains(&String::new()),
            "blank lines survive wrapping"
        );
        assert!(lines.iter().all(|s| s
            .chars()
            .map(|c| if c == '結' || c == '論' { 2 } else { 1 })
            .sum::<usize>()
            <= 5));
        assert_eq!(wrap_measured("αβ", 0.0, |_| 1.0), vec!["α", "β"]);
    }

    #[test]
    fn proof_pages_reach_every_line_and_clamp_after_resize() {
        let mut visible = Vec::new();
        let mut page = ProofPage::new(37, 0, 14);
        loop {
            visible.extend(page.start..page.end);
            let Some(next) = page.next() else { break };
            page = ProofPage::new(37, next, 14);
        }
        assert_eq!(visible, (0..37).collect::<Vec<_>>());
        assert_eq!(page.previous(), Some(1));
        assert_eq!(ProofPage::new(5, page.index, 14).index, 0);
        assert_eq!(ProofPage::new(0, usize::MAX, 0).end, 0);
    }
}
