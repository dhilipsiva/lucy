//! Deterministic hashed retrieval. A changed corpus invalidates every IDF-weighted vector.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const DIMS: usize = 384;
pub const FLOOR: f32 = 0.12;
pub fn fnv(text: &str) -> u32 {
    text.encode_utf16()
        .fold(0x811c9dc5, |h, c| (h ^ c as u32).wrapping_mul(0x01000193))
}
pub fn chunks(text: &str) -> Vec<String> {
    let chars: Vec<_> = text.chars().collect();
    chars.chunks(320).map(|s| s.iter().collect()).collect()
}
fn features(text: &str) -> BTreeMap<u32, f32> {
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|s| s.chars().count() >= 2)
        .collect();
    let mut f = BTreeMap::new();
    let mut add = |s: String, w: f32| {
        *f.entry(fnv(&s)).or_default() += w;
    };
    for t in &tokens {
        add(format!("w:{t}"), 1.0);
        if t.chars().count() >= 3 {
            let chars: Vec<_> = format!("^{t}$").chars().collect();
            for tri in chars.windows(3) {
                add(format!("c:{}", tri.iter().collect::<String>()), 0.35);
            }
        }
    }
    for pair in tokens.windows(2) {
        add(format!("2:{}_{}", pair[0], pair[1]), 1.25);
    }
    f
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashSpace {
    pub fingerprint: String,
    pub texts: Vec<String>,
    pub parents: Vec<usize>,
    df: BTreeMap<u32, usize>,
    pub vectors: Vec<Vec<f32>>,
}
impl HashSpace {
    pub fn build(texts: &[String]) -> Self {
        let mut s = Self::empty(texts);
        for (i, text) in texts.iter().enumerate() {
            s.add_document(text, i);
        }
        s.finish();
        s
    }
    fn empty(texts: &[String]) -> Self {
        Self {
            fingerprint: lucy_room_memory::digest(serde_json::to_string(texts).unwrap().as_bytes()),
            texts: vec![],
            parents: vec![],
            df: BTreeMap::new(),
            vectors: vec![],
        }
    }
    fn add_document(&mut self, text: &str, parent: usize) {
        for c in chunks(text) {
            for k in features(&c).keys() {
                *self.df.entry(*k).or_default() += 1;
            }
            self.texts.push(c);
            self.parents.push(parent);
        }
    }
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0; DIMS];
        for (k, w) in features(text) {
            let idf = ((self.texts.len() as f32 + 1.0)
                / (*self.df.get(&k).unwrap_or(&0) as f32 + 1.0))
                .ln()
                + 1.0;
            v[(k as usize) % DIMS] += w * idf * if k & 0x80000000 == 0 { 1.0 } else { -1.0 };
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
    fn finish(&mut self) {
        self.vectors = self.texts.iter().map(|t| self.embed(t)).collect();
    }
    pub async fn build_yielding(texts: &[String]) -> Self {
        let mut s = Self::empty(texts);
        for (i, text) in texts.iter().enumerate() {
            s.add_document(text, i);
            if i % 40 == 0 {
                crate::journal::yield_now().await;
            }
        }
        for i in 0..s.texts.len() {
            s.vectors.push(s.embed(&s.texts[i]));
            if i % 40 == 0 {
                crate::journal::yield_now().await;
            }
        }
        s
    }
    pub fn search(&self, query: &str, limit: usize) -> Vec<(usize, f32, String)> {
        let q = self.embed(query);
        let mut scores: BTreeMap<usize, (f32, String)> = BTreeMap::new();
        for (i, v) in self.vectors.iter().enumerate() {
            let score = q.iter().zip(v).map(|(a, b)| a * b).sum::<f32>();
            if score >= FLOOR {
                let entry = scores
                    .entry(self.parents[i])
                    .or_insert((0.0, String::new()));
                if score > entry.0 {
                    *entry = (score, self.texts[i].clone());
                }
            }
        }
        let mut out: Vec<_> = scores.into_iter().map(|(i, (s, t))| (i, s, t)).collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        out.truncate(limit);
        out
    }
    pub fn valid_for(&self, texts: &[String]) -> bool {
        self.fingerprint == Self::empty(texts).fingerprint
            && self.vectors.len() == self.texts.len()
            && self.parents.len() == self.texts.len()
            && self.parents.iter().all(|p| *p < texts.len())
            && self
                .vectors
                .iter()
                .all(|v| v.len() == DIMS && v.iter().all(|x| x.is_finite()))
    }
}
pub fn topic_counts(topics: impl Iterator<Item = Vec<String>>) -> Vec<(String, usize)> {
    let mut map: BTreeMap<String, usize> = BTreeMap::new();
    for tags in topics {
        for t in tags.into_iter().collect::<BTreeSet<_>>() {
            *map.entry(t).or_default() += 1;
        }
    }
    let mut out: Vec<_> = map.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_chunks_and_search() {
        let text = "🐦".repeat(800);
        assert!(chunks(&text).iter().all(|s| s.chars().count() <= 320));
        assert_eq!(chunks(&text).concat(), text);
        let items = vec![
            "Luffy and Sabo in One Piece".into(),
            "NixOS system configuration".into(),
        ];
        let s = HashSpace::build(&items);
        assert_eq!(s.search("One Piece Luffy", 5)[0].0, 0);
        assert!(s.valid_for(&items));
        assert!(!s.valid_for(&[items[0].clone()]));
    }
}
