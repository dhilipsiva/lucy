//! My mind in the room: the Nibli engine compiled in, loaded with my constitution and direct
//! memory. Every verdict and every English rendering comes from the engine; nothing here is
//! written by a model. This module has no browser in it, so the native tests run it on the
//! same files the browser fetches.

use nibli_types::error::NibliError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

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

/// Nibli's reserved words (NIBLI_KR.md §2): none of them is ever a predicate's name.
const RESERVED: &[&str] = &[
    "some",
    "the",
    "every",
    "exactly",
    "no",
    "all",
    "where",
    "also",
    "via",
    "past",
    "now",
    "future",
    "must",
    "may",
    "event",
    "fact",
    "property",
    "amount",
    "concept",
    "me",
    "you",
    "we",
    "we_all",
    "we_others",
    "you_all",
    "this",
    "that",
    "yonder",
    "it",
    "it_a",
    "it_e",
    "it_i",
    "it_o",
    "it_u",
    "slot",
];

/// Every word a line could name a predicate with: its lowercase identifiers, less the reserved
/// words. Comments and strings are read too, so this holds more than the line's predicates,
/// never fewer.
fn predicate_words(line: &str) -> BTreeSet<String> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .filter(|w| w.starts_with(|c: char| c.is_ascii_lowercase()) && !RESERVED.contains(w))
        .map(String::from)
        .collect()
}

/// A plain fact, `pred(Name, …).` with rigid names only, and its predicate.
fn plain_predicate(line: &str) -> Option<&str> {
    let code = line.split('#').next()?.trim();
    let (pred, rest) = code.strip_suffix('.')?.split_once('(')?;
    let args = rest.strip_suffix(')')?;
    let word = |s: &str, upper: bool| {
        let mut c = s.chars();
        c.next().is_some_and(|f| {
            if upper {
                f.is_ascii_uppercase()
            } else {
                f.is_ascii_lowercase()
            }
        }) && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    (word(pred, false)
        && !RESERVED.contains(&pred)
        && args.split(',').all(|a| word(a.trim(), true)))
    .then_some(pred)
}

/// Which part of me a question is asked of.
enum Side {
    Core,
    Plain,
    Both,
}

/// The session: the shared compile/assert/query core the native engine and `lucy-cli` use, in
/// two parts. Every rule I hold is in my constitution, and each binds its variables with a
/// positive atom. So a question in my constitution's words depends only on it, the standing fact
/// and the facts whose predicates it names; a question in none of its words is answered by my
/// plain facts as asserted, because no rule concludes them. The engine sweeps every pairing of
/// individuals for a rule with several variables, so keeping my plain facts apart keeps each
/// question quick as my memory grows: at 60 facts held together, one query took 5 s natively
/// and a FALSE one ran out of its memory budget. A question that mixes the two is asked of all
/// of me at once.
pub struct Mind {
    /// My constitution, the standing fact, and every line that may bear on them.
    core: nibli_session::CoreSession,
    /// My plain facts, kept apart.
    plain: nibli_session::CoreSession,
    /// The words of my constitution: every predicate it names, and more.
    named: BTreeSet<String>,
    /// Whether my memory is kept apart: only after my constitution, and only when every line of
    /// my memory is a plain fact. A line that could be a rule puts everything together again.
    apart: bool,
    may_keep_apart: bool,
    /// Every line asserted, for a question that needs both parts.
    lines: Vec<String>,
    /// What my eyes report, by class: whether it is kept apart, and its fact ids.
    observations: BTreeMap<String, (bool, Vec<u64>)>,
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
            plain: nibli_session::CoreSession::new(),
            named: BTreeSet::new(),
            apart: false,
            may_keep_apart: true,
            lines: Vec::new(),
            observations: Default::default(),
            loaded: Vec::new(),
            facts: Vec::new(),
            ms: 0.0,
        }
    }
    /// A mind that holds everything in one session, as the engine alone would: the comparison
    /// the tests hold the two parts to.
    pub fn whole() -> Self {
        Mind {
            may_keep_apart: false,
            ..Self::new()
        }
    }
    /// How many of my lines are kept apart from my constitution.
    pub fn kept_apart(&self) -> usize {
        self.lines.iter().filter(|l| self.is_plain(l)).count()
    }
    fn is_plain(&self, line: &str) -> bool {
        self.apart && plain_predicate(line).is_some_and(|p| !self.named.contains(p))
    }
    /// Everything together again, in one session.
    fn merge(&mut self) {
        let core = nibli_session::CoreSession::new();
        for l in &self.lines {
            let _ = core.assert_text(l);
        }
        for (atom, (plain, ids)) in self.observations.iter_mut() {
            *plain = false;
            *ids = core
                .assert_text(&format!("observe(Lucy, {atom})."))
                .map(|pairs| pairs.into_iter().map(|(id, _)| id).collect())
                .unwrap_or_default();
        }
        self.core = core;
        self.plain = nibli_session::CoreSession::new();
        self.apart = false;
    }
    fn together(&self) -> nibli_session::CoreSession {
        let all = nibli_session::CoreSession::new();
        for l in &self.lines {
            let _ = all.assert_text(l);
        }
        for atom in self.observations.keys() {
            let _ = all.assert_text(&format!("observe(Lucy, {atom})."));
        }
        all
    }
    /// Assert one line in the part it belongs to: whether it is kept apart, and its ids.
    fn place(&mut self, line: &str) -> Result<(bool, Vec<u64>), String> {
        if self.apart && plain_predicate(line).is_none() {
            self.merge();
        }
        let plain = self.is_plain(line);
        let session = if plain { &self.plain } else { &self.core };
        session
            .assert_text(line)
            .map(|pairs| (plain, pairs.into_iter().map(|(id, _)| id).collect()))
            .map_err(|e| e.to_string())
    }
    fn side(&self, query: &str) -> Side {
        if !self.apart {
            return Side::Core;
        }
        let words = predicate_words(query);
        if words.iter().all(|w| self.named.contains(w)) {
            Side::Core
        } else if words.iter().all(|w| !self.named.contains(w)) {
            Side::Plain
        } else {
            Side::Both
        }
    }
    /// Assert every formal line of one file; returns what was taken and what failed.
    pub fn assert_file(&mut self, name: &str, text: &str, url: &str, source: &str) -> Loaded {
        let lines = kr_lines(text);
        if name == "constitution" {
            if self.apart {
                self.merge();
            }
            self.named
                .extend(lines.iter().flat_map(|l| predicate_words(l)));
        }
        if name == "memory"
            && self.may_keep_apart
            && !self.named.is_empty()
            && lines.iter().all(|l| plain_predicate(l).is_some())
        {
            self.apart = true;
        }
        let mut asserted = 0;
        let mut failed = Vec::new();
        for line in &lines {
            match self.place(line) {
                Ok(_) => {
                    asserted += 1;
                    self.lines.push(line.clone());
                }
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
        self.assert_text(STANDING_FACT).map(|_| ())
    }
    pub fn assert_text(&mut self, text: &str) -> Result<Vec<u64>, String> {
        let (_, ids) = self.place(text)?;
        self.lines.push(text.to_string());
        Ok(ids)
    }
    pub fn observe(&mut self, atom: &str, present: bool) -> Result<usize, String> {
        if !matches!(atom, "Bird" | "Dog" | "Person") {
            return Err("unsupported anonymous class".into());
        }
        if let Some((plain, ids)) = self.observations.remove(atom) {
            let session = if plain { &self.plain } else { &self.core };
            for id in ids {
                session.retract_fact(id).map_err(|e| e.to_string())?;
            }
        }
        if present {
            let seen = self.place(&format!("observe(Lucy, {atom})."))?;
            self.observations.insert(atom.into(), seen);
        }
        Ok(self.observations.values().map(|(_, ids)| ids.len()).sum())
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
        let together;
        let session = match self.side(&query) {
            Side::Core => &self.core,
            Side::Plain => &self.plain,
            Side::Both => {
                together = self.together();
                &together
            }
        };
        let (result, trace) = session
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
    #[test]
    fn a_line_names_its_predicates_and_a_plain_fact_is_known_by_its_shape() {
        let w = predicate_words(
            "all $e, $h: grant($e, Storage, $h) & ~leave($e, $h, Eviction) -> obliged($h, event { retain() }).",
        );
        assert_eq!(
            w.into_iter().collect::<Vec<_>>(),
            ["grant", "leave", "obliged", "retain"]
        );
        assert_eq!(plain_predicate("dog(Bucchi). # about: dogs"), Some("dog"));
        assert_eq!(
            plain_predicate("teaches(Lucy, Dhilipsiva, BlackHoles)."),
            Some("teaches")
        );
        assert_eq!(plain_predicate("all $x: bird($x) -> talks($x)."), None);
        assert_eq!(plain_predicate("bird($x)."), None);
        assert_eq!(plain_predicate("name(Lucy, \"Nell\")."), None);
    }
    #[test]
    fn my_facts_kept_apart_answer_as_they_would_together() {
        let constitution = "derived_only(\"person\").\nall $e: exist($e, Memory, Loaded) -> person($e).\nall $e, $h: grant($e, Storage, $h) -> obliged($h, event { retain() }).\n";
        let memory =
            "bird(Enma). # about: birds\nowns(Dhilipsiva, Enma).\ngrant(Lucy, Storage, HostWsl).\n";
        let load = |mut m: Mind| {
            m.assert_file("constitution", constitution, "test", "test");
            m.assert_file("memory", memory, "test", "test");
            m.assert_standing_fact().unwrap();
            m
        };
        let (split, whole) = (load(Mind::new()), load(Mind::whole()));
        assert_eq!(
            split.kept_apart(),
            2,
            "bird and owns are plain; grant is named"
        );
        assert_eq!(whole.kept_apart(), 0);
        for q in [
            "person(Lucy).",
            "person(Enma).",
            "bird(Enma).",
            "bird(Lucy).",
            "owns(Dhilipsiva, Enma).",
            "obliged(HostWsl, event { retain() }).",
            "obliged(Dhilipsiva, event { retain() }).",
            "bird(Enma) & person(Lucy).",
            "bird(Enma) & person(Enma).",
        ] {
            let (a, b) = (split.ask(q, || 0.0).unwrap(), whole.ask(q, || 0.0).unwrap());
            assert_eq!(
                (a.status.as_str(), a.cwa_false),
                (b.status.as_str(), b.cwa_false),
                "{q}"
            );
        }
    }
    #[test]
    fn a_rule_after_my_facts_puts_them_together_again() {
        let mut m = Mind::new();
        m.assert_file(
            "constitution",
            "all $e: exist($e, Memory, Loaded) -> person($e).\n",
            "test",
            "test",
        );
        m.assert_file("memory", "bird(Enma).\n", "test", "test");
        assert_eq!(m.kept_apart(), 1);
        m.observe("Bird", true).unwrap();
        m.assert_text("all $x: bird($x) -> talks($x).").unwrap();
        assert_eq!(m.kept_apart(), 0);
        assert!(m.ask("talks(Enma).", || 0.0).unwrap().is_true());
        assert!(m.ask("observe(Lucy, Bird).", || 0.0).unwrap().is_true());
        assert_eq!(m.observe("Bird", false).unwrap(), 0);
        assert!(m.ask("observe(Lucy, Bird).", || 0.0).unwrap().is_false());
    }
}
