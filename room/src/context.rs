//! The evidence a model writes my reply from (roadmap/N9.md): the facts of my direct memory that
//! share a word with the question (all of a subject's facts when one is chosen), the engine's
//! verdict when one is on the board, what recall found, and the last turns of this visit; each
//! trimmed to a budget that fits the model. One builder for every backend.

use crate::cast::Entity;
use crate::mind::{english_of, Fact};

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub evidence: usize,
    pub history: usize,
    pub turns: usize,
}
/// My small model in the browser: its 2,048-token window (model.rs trims further).
pub const WORDS: Budget = Budget {
    evidence: 6000,
    history: 2000,
    turns: 4,
};
/// A hosted model or a local Ollama: larger windows.
pub const LARGE: Budget = Budget {
    evidence: 24000,
    history: 8000,
    turns: 12,
};

/// Cut to at most `max` bytes on a character boundary, marking the cut.
pub fn trim(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[…trimmed]", &text[..end])
}

/// My facts that bear on a question: a subject's own facts, and any fact sharing a word with it.
pub fn direct(question: &str, facts: &[Fact], subject: Option<&Entity>) -> String {
    let words: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect();
    facts
        .iter()
        .filter(|f| {
            subject.is_some_and(|e| e.facts.iter().any(|g| g.kr == f.kr))
                || words
                    .iter()
                    .any(|w| f.kr.to_lowercase().contains(w.as_str()))
        })
        .map(|f| format!("[direct memory] {} ({})", f.kr, english_of(&f.kr)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The evidence block: direct memory, the verdict on the board, recalled records.
pub fn evidence(direct: &str, verdict: &str, recalled: &str, budget: Budget) -> String {
    let parts: Vec<&str> = [direct, verdict, recalled]
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect();
    trim(&parts.join("\n\n"), budget.evidence)
}

/// The last turns of this visit as (role, text), oldest first, within the budget.
pub fn turns<'a>(
    messages: impl DoubleEndedIterator<Item = (&'a str, &'a str)>,
    budget: Budget,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    let mut used = 0;
    for (role, text) in messages.rev().take(budget.turns) {
        if used + text.len() > budget.history {
            break;
        }
        used += text.len();
        out.push((role.to_string(), text.to_string()));
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mind::fact_lines;
    #[test]
    fn a_subject_brings_its_facts_and_words_bring_theirs() {
        let facts = fact_lines("bird(Enma).\nowns(Dhilipsiva, Enma).\ndog(Bucchi).\n");
        let p = crate::cast::presences(&facts);
        let enma = p.iter().find(|e| e.name == "Enma").unwrap();
        let d = direct("what does she say", &facts, Some(enma));
        assert!(
            d.contains("bird(Enma).")
                && d.contains("owns(Dhilipsiva, Enma).")
                && !d.contains("Bucchi")
        );
        assert!(direct("tell me about bucchi", &facts, None).contains("dog(Bucchi)."));
    }
    #[test]
    fn budgets_hold() {
        assert_eq!(
            trim("é".repeat(10).as_str(), 5)
                .chars()
                .filter(|c| *c == 'é')
                .count(),
            2
        );
        let msgs = [("user", "aaaa"), ("assistant", "bbbb"), ("user", "cccc")];
        let t = turns(
            msgs.into_iter(),
            Budget {
                evidence: 0,
                history: 8,
                turns: 12,
            },
        );
        assert_eq!(
            t,
            vec![
                ("assistant".into(), "bbbb".into()),
                ("user".into(), "cccc".into())
            ]
        );
    }
}
