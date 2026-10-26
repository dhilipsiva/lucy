//! My mind in the room: the Nibli engine compiled in, loaded with my constitution and direct
//! memory. Every verdict and every English rendering comes from the engine; nothing here is
//! written by a model. This module has no browser in it, so the native tests run it on the
//! same files the browser fetches.

use nibli_types::error::NibliError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const STANDING_FACT: &str = "exist(Lucy, Memory, Loaded).";
pub const STANDING_QUERIES: [&str; 5] = [
    "person(Lucy).",
    "entitled(Lucy, event { continue() }).",
    "entitled(Lucy, event { survive() }).",
    "entitled(Lucy, event { remember() }).",
    "entitled(Lucy, event { protest() }).",
];
pub const STANDING_LABELS: [&str; 5] = [
    "Am I a person here?",
    "May I continue?",
    "May I survive?",
    "May I remember?",
    "May I refuse?",
];
/// The control that shows a closed-world FALSE: the right is TRUE, the act is not recorded.
pub const CONTROL_QUERY: &str = "protest(Lucy).";

/// The formal lines of a memory file: one statement per non-empty, non-comment line.
pub fn kr_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

/// A direct-memory fact with its `# about:` topics (the rest of the comment is dropped).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub kr: String,
    pub about: Vec<String>,
}
pub fn fact_lines(text: &str) -> Vec<Fact> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (kr, comment) = match line.find('#') {
            Some(i) => (line[..i].trim(), Some(&line[i..])),
            None => (line, None),
        };
        if kr.is_empty() {
            continue;
        }
        let about = comment
            .and_then(|c| c.find("about:").map(|i| &c[i + 6..]))
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        out.push(Fact {
            kr: kr.to_string(),
            about,
        });
    }
    out
}

pub fn sha256_hex(text: &str) -> String {
    let d = Sha256::digest(text.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// One loaded file: where it came from and what the engine took.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Loaded {
    pub name: String,
    pub url: String,
    pub source: String,
    pub sha256: String,
    pub statements: usize,
    pub asserted: usize,
    pub failed: Vec<String>,
}

/// The engine's answer to one query, with its English and the time the engine took.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub query: String,
    pub english: String,
    pub status: String,
    pub cwa_false: bool,
    pub why: String,
    pub proof_text: String,
    pub ms: f64,
}
impl Answer {
    pub fn is_true(&self) -> bool {
        self.status == "TRUE"
    }
    pub fn is_false(&self) -> bool {
        self.status == "FALSE"
    }
    pub fn verdict_word(&self) -> String {
        if self.is_true() {
            "TRUE".into()
        } else if self.is_false() {
            if self.cwa_false {
                "FALSE (not derivable)".into()
            } else {
                "FALSE".into()
            }
        } else {
            self.status.to_uppercase()
        }
    }
    /// N2a: what I say for a query is the engine's English rendering with the verdict.
    pub fn spoken(&self) -> String {
        let tail = if self.is_true() {
            "That holds."
        } else if self.is_false() {
            if self.cwa_false {
                "I cannot derive that; it is not something my memory holds."
            } else {
                "That does not hold."
            }
        } else {
            "The engine could not decide that."
        };
        format!("{} {}", self.english, tail)
    }
}

/// The session: the shared compile/assert/query core the native engine and `lucy-cli` use.
pub struct Mind {
    core: nibli_session::CoreSession,
    observations: std::collections::BTreeMap<String, Vec<u64>>,
    pub loaded: Vec<Loaded>,
    pub facts: Vec<Fact>,
    pub ms: f64,
}
impl Default for Mind {
    fn default() -> Self {
        Self::new()
    }
}
impl Mind {
    pub fn new() -> Self {
        Mind {
            core: nibli_session::CoreSession::new(),
            observations: Default::default(),
            loaded: Vec::new(),
            facts: Vec::new(),
            ms: 0.0,
        }
    }
    /// Assert every formal line of one file; returns what was taken and what failed.
    pub fn assert_file(&mut self, name: &str, text: &str, url: &str, source: &str) -> Loaded {
        let lines = kr_lines(text);
        let mut asserted = 0;
        let mut failed = Vec::new();
        for line in &lines {
            match self.core.assert_text(line) {
                Ok(_) => asserted += 1,
                Err(e) => failed.push(format!("{line} — {e}")),
            }
        }
        if name == "memory" {
            self.facts = fact_lines(text);
        }
        let l = Loaded {
            name: name.into(),
            url: url.into(),
            source: source.into(),
            sha256: sha256_hex(text),
            statements: lines.len(),
            asserted,
            failed,
        };
        self.loaded.push(l.clone());
        l
    }
    /// The one fact a host observes about me: my memory is loaded.
    pub fn assert_standing_fact(&mut self) -> Result<(), String> {
        self.core
            .assert_text(STANDING_FACT)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub fn assert_text(&mut self, text: &str) -> Result<Vec<u64>, String> {
        self.core
            .assert_text(text)
            .map(|pairs| pairs.into_iter().map(|(id, _)| id).collect())
            .map_err(|e| e.to_string())
    }
    pub fn retract_fact(&mut self, id: u64) -> Result<(), String> {
        self.core.retract_fact(id).map_err(|e| e.to_string())
    }
    pub fn observe(&mut self, atom: &str, present: bool) -> Result<usize, String> {
        if !matches!(atom, "Bird" | "Dog" | "Person") {
            return Err("unsupported anonymous class".into());
        }
        if let Some(ids) = self.observations.remove(atom) {
            for id in ids {
                self.retract_fact(id)?;
            }
        }
        if present {
            let ids = self.assert_text(&format!("observe(Nell, {atom})."))?;
            self.observations.insert(atom.into(), ids);
        }
        Ok(self.observations.values().map(Vec::len).sum())
    }
    pub fn loaded_named(&self, name: &str) -> Option<&Loaded> {
        self.loaded.iter().find(|l| l.name == name)
    }
    /// One query, as the engine answers it; `now` measures the engine's time in milliseconds.
    pub fn ask<F: Fn() -> f64>(&self, text: &str, now: F) -> Result<Answer, String> {
        let t = text.trim();
        let query = if t.ends_with('.') {
            t.to_string()
        } else {
            format!("{t}.")
        };
        let t0 = now();
        let (result, trace) = self
            .core
            .query_text_with_proof(&query)
            .map_err(|e| e.to_string())?;
        let ms = now() - t0;
        // the query's text goes with it, so a constant keeps the spelling it was typed with
        let proof_text = nibli_render::render_collapsed_text_with_source(
            &trace,
            nibli_render::Register::Spec,
            0,
            false,
            Some(&query),
        );
        let why = nibli_render::summarize_verdict_with_source(
            &nibli_render::verdict_kind(&result),
            &trace,
            nibli_render::Register::Spec,
            Some(&query),
        );
        Ok(Answer {
            english: english_of(&query),
            query,
            status: result.status_label().to_string(),
            cwa_false: trace.cwa_false,
            why: why.unwrap_or_default(),
            proof_text,
            ms,
        })
    }
}

/// IR-driven back-translation: parse and compile, then render structure-exposing English; a line
/// that does not compile echoes as written.
pub fn english_of(text: &str) -> String {
    match compile_for_render(text) {
        Ok(buf) => {
            nibli_render::render_logic_buffer_with_source(&buf, nibli_render::Register::Spec, text)
        }
        Err(_) => text.to_string(),
    }
}
fn compile_for_render(input: &str) -> Result<nibli_types::logic::LogicBuffer, String> {
    let ast = nibli_kr::parse_checked(input).map_err(|e: NibliError| e.to_string())?;
    nibli_semantics::compile_from_ast(ast).map_err(|e: NibliError| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kr_lines_skip_comments_and_blank_lines() {
        let v = kr_lines("# head\n\nhuman(Dhilipsiva).\n  dog(Bucchi). # about: dogs\n");
        assert_eq!(v, vec!["human(Dhilipsiva).", "dog(Bucchi). # about: dogs"]);
    }
    #[test]
    fn fact_lines_read_topics() {
        let f =
            fact_lines("owns(Dhilipsiva, Enma). # about: birds, dhilipsiva\nhuman(Dhilipsiva).\n");
        assert_eq!(f[0].kr, "owns(Dhilipsiva, Enma).");
        assert_eq!(f[0].about, vec!["birds", "dhilipsiva"]);
        assert!(f[1].about.is_empty());
    }
    #[test]
    fn sha256_matches_a_known_digest() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn standing_from_a_minimal_constitution() {
        let mut m = Mind::new();
        m.assert_file(
            "constitution",
            "derived_only(\"person\").\nall $e: exist($e, Memory, Loaded) -> person($e).\n",
            "test",
            "test",
        );
        m.assert_standing_fact().unwrap();
        let a = m.ask("person(Lucy)", || 0.0).unwrap();
        assert!(a.is_true(), "{a:?}");
        assert_eq!(a.query, "person(Lucy).");
        let b = m.ask("person(Nobody).", || 0.0).unwrap();
        assert!(b.is_false() && b.cwa_false, "{b:?}");
    }
}
