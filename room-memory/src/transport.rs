//! Bounded, byte-preserving memory transport shared by native and browser readers.
//! A part is a UTF-8 fragment, not necessarily a complete Nibli statement.
use crate::digest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const PART_BYTES: usize = 4 * 1024 * 1024;
pub const MANIFEST_BYTES: usize = 1024 * 1024;
pub const LEGACY_BYTES: usize = 100 * 1024 * 1024;
pub const ARCHIVE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PARTS: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub bytes: usize,
    pub sha256: String,
    pub parts: Vec<Part>,
}
#[derive(Clone, Debug)]
pub struct Manifest {
    pub version: u64,
    artifacts: BTreeMap<String, Archive>,
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn repository(s: &str) -> bool {
    let parts: Vec<_> = s.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !matches!(*p, "." | "..")
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}
impl Manifest {
    pub fn parse(text: &str, repo: &str, scope: &str, public: bool) -> Result<Self, String> {
        if text.len() > MANIFEST_BYTES {
            return Err("manifest exceeds 1 MiB limit".into());
        }
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if !repository(repo)
            || value["repository"] != repo
            || value["scope"] != scope
            || value["public"] != public
        {
            return Err("remote manifest identity or scope mismatch".into());
        }
        let version = value["version"]
            .as_u64()
            .ok_or("missing manifest version")?;
        let mut artifacts = BTreeMap::new();
        match version {
            1 => {
                // Old readers required only the hash of the archive they actually read.
                for name in ["operations", "snapshot"] {
                    let path = format!("{name}.nibli");
                    if let Some(sha256) = value["hashes"][&path].as_str() {
                        if !hash(sha256) {
                            return Err("archive hash mismatch: invalid digest".into());
                        }
                        artifacts.insert(
                            name.into(),
                            Archive {
                                bytes: 0,
                                sha256: sha256.into(),
                                parts: vec![Part {
                                    path,
                                    bytes: LEGACY_BYTES,
                                    sha256: sha256.into(),
                                }],
                            },
                        );
                    }
                }
            }
            2 => {
                if value["schema"] != "lucy-operations-v1"
                    || value["ref"] != "lucy-memory"
                    || value["identity"].as_str().is_none_or(str::is_empty)
                {
                    return Err("unsupported memory schema or identity".into());
                }
                for name in ["operations", "snapshot"] {
                    let archive: Archive = serde_json::from_value(value["artifacts"][name].clone())
                        .map_err(|e| format!("invalid {name} descriptor: {e}"))?;
                    if archive.bytes > ARCHIVE_BYTES
                        || !hash(&archive.sha256)
                        || archive.parts.is_empty()
                        || archive.parts.len() > MAX_PARTS
                    {
                        return Err("archive descriptor exceeds limits or has invalid hash".into());
                    }
                    let mut total = 0usize;
                    for p in &archive.parts {
                        if !hash(&p.sha256)
                            || p.path != format!("parts/{}.txt", p.sha256)
                            || p.bytes > PART_BYTES
                        {
                            return Err("invalid part path, size or hash".into());
                        }
                        total = total.checked_add(p.bytes).ok_or("archive size overflow")?;
                    }
                    if total != archive.bytes {
                        return Err("archive size mismatch".into());
                    }
                    artifacts.insert(name.into(), archive);
                }
            }
            _ => {
                return Err(format!(
                    "unsupported memory manifest version {version}; update Lucy"
                ))
            }
        }
        Ok(Self { version, artifacts })
    }
    pub fn artifact(&self, name: &str) -> Result<&Archive, String> {
        self.artifacts
            .get(name)
            .ok_or_else(|| format!("manifest lacks {name} archive"))
    }
    pub fn assembler(&self, name: &str) -> Result<Assembler<'_>, String> {
        Ok(Assembler {
            archive: self.artifact(name)?,
            legacy: self.version == 1,
            text: String::new(),
            next: 0,
        })
    }
}
pub struct Assembler<'a> {
    archive: &'a Archive,
    legacy: bool,
    text: String,
    next: usize,
}
impl Assembler<'_> {
    pub fn push(&mut self, text: &str) -> Result<(), String> {
        let p = self
            .archive
            .parts
            .get(self.next)
            .ok_or("unexpected archive part")?;
        if text.len() > p.bytes || (!self.legacy && text.len() != p.bytes) {
            return Err("part byte length mismatch".into());
        }
        if digest(text.as_bytes()) != p.sha256 {
            return Err("part hash mismatch".into());
        }
        let max = if self.legacy {
            LEGACY_BYTES
        } else {
            ARCHIVE_BYTES
        };
        if self.text.len().saturating_add(text.len()) > max {
            return Err("archive exceeds byte limit".into());
        }
        self.text.push_str(text);
        self.next += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<String, String> {
        if self.next != self.archive.parts.len()
            || (!self.legacy && self.text.len() != self.archive.bytes)
        {
            return Err("incomplete archive".into());
        }
        if digest(self.text.as_bytes()) != self.archive.sha256 {
            return Err("archive hash mismatch".into());
        }
        Ok(self.text)
    }
}

pub struct Bundle {
    pub version: u64,
    pub files: Vec<(String, String)>,
}
/// Metadata comes from local enrollment, never a fetched manifest's keys or routes.
pub fn bundle(
    mut metadata: Value,
    operations: String,
    snapshot: String,
    previous: u64,
) -> Result<Bundle, String> {
    let version = if previous == 2 || operations.len().max(snapshot.len()) > PART_BYTES {
        2
    } else {
        1
    };
    let mut files = Vec::new();
    metadata["version"] = json!(version);
    if version == 1 {
        metadata["paths"] = json!({"operations":"operations.nibli","snapshot":"snapshot.nibli"});
        metadata["hashes"] = json!({"operations.nibli":digest(operations.as_bytes()),"snapshot.nibli":digest(snapshot.as_bytes())});
        files.push(("operations.nibli".into(), operations));
        files.push(("snapshot.nibli".into(), snapshot));
    } else {
        let mut parts = BTreeMap::new();
        for (name, text) in [("operations", operations), ("snapshot", snapshot)] {
            if text.len() > ARCHIVE_BYTES {
                return Err(format!(
                    "{name} exceeds 256 MiB archive limit; outbox retained"
                ));
            }
            let mut archive = Archive {
                bytes: text.len(),
                sha256: digest(text.as_bytes()),
                parts: Vec::new(),
            };
            let mut start = 0;
            loop {
                let mut end = text.len().min(start + PART_BYTES);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let fragment = &text[start..end];
                let sha256 = digest(fragment.as_bytes());
                let path = format!("parts/{sha256}.txt");
                archive.parts.push(Part {
                    path: path.clone(),
                    bytes: fragment.len(),
                    sha256,
                });
                parts.entry(path).or_insert_with(|| fragment.to_owned());
                start = end;
                if start == text.len() {
                    break;
                }
            }
            metadata["artifacts"][name] =
                serde_json::to_value(archive).map_err(|e| e.to_string())?;
        }
        files.extend(parts);
    }
    let manifest = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    if manifest.len() > MANIFEST_BYTES {
        return Err("manifest exceeds 1 MiB limit".into());
    }
    files.push(("manifest.json".into(), manifest));
    Ok(Bundle { version, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> Value {
        json!({"identity":"test","repository":"test/memory","scope":"test","public":true,"ref":"lucy-memory","schema":"lucy-operations-v1"})
    }
    fn parse(b: &Bundle) -> Manifest {
        Manifest::parse(&b.files.last().unwrap().1, "test/memory", "test", true).unwrap()
    }
    #[test]
    fn legacy_and_chunked_reconstruct_exact_unicode_and_long_lines() {
        for previous in [1, 2] {
            let text = format!(
                "{}🐦\n{}",
                "x".repeat(PART_BYTES - 1),
                "é".repeat(PART_BYTES)
            );
            let b = bundle(metadata(), text.clone(), "small".into(), previous).unwrap();
            assert_eq!(b.version, 2);
            let m = parse(&b);
            let files: BTreeMap<_, _> = b.files.iter().cloned().collect();
            let mut a = m.assembler("operations").unwrap();
            for p in &m.artifact("operations").unwrap().parts {
                a.push(&files[&p.path]).unwrap();
            }
            assert_eq!(a.finish().unwrap(), text);
        }
        let b = bundle(metadata(), "hello".into(), "snapshot".into(), 1).unwrap();
        assert_eq!(b.version, 1);
        let m = parse(&b);
        let mut a = m.assembler("operations").unwrap();
        a.push("hello").unwrap();
        assert_eq!(a.finish().unwrap(), "hello");
        assert_eq!(
            bundle(metadata(), "".into(), "".into(), 2).unwrap().version,
            2
        );
    }
    #[test]
    fn corrupt_missing_reordered_parts_and_malicious_descriptors_fail() {
        let b = bundle(
            metadata(),
            format!("{}end", "x".repeat(PART_BYTES)),
            "small".into(),
            2,
        )
        .unwrap();
        let m = parse(&b);
        assert!(m.assembler("operations").unwrap().finish().is_err());
        assert!(m.assembler("operations").unwrap().push("end").is_err());
        assert!(m.assembler("snapshot").unwrap().push("wrong").is_err());
        let original: Value = serde_json::from_str(&b.files.last().unwrap().1).unwrap();
        for (field, value) in [
            ("path", json!("../private")),
            ("bytes", json!(PART_BYTES + 1)),
            ("sha256", json!("bad")),
        ] {
            let mut bad = original.clone();
            bad["artifacts"]["operations"]["parts"][0][field] = value;
            assert!(Manifest::parse(&bad.to_string(), "test/memory", "test", true).is_err());
        }
        let mut bad = original.clone();
        bad["artifacts"]["operations"]["bytes"] = json!(ARCHIVE_BYTES + 1);
        assert!(Manifest::parse(&bad.to_string(), "test/memory", "test", true).is_err());
        assert!(Manifest::parse(&original.to_string(), "test/memory", "test", false).is_err());
    }
    #[test]
    fn archive_above_old_64_mib_limit_roundtrips() {
        let text = format!("{}🐦", "0123456789abcdef".repeat(4 * 1024 * 1024 + 1));
        let b = bundle(metadata(), text.clone(), "snapshot".into(), 1).unwrap();
        let m = parse(&b);
        let files: BTreeMap<_, _> = b.files.iter().map(|(path, text)| (path, text)).collect();
        let mut a = m.assembler("operations").unwrap();
        for p in &m.artifact("operations").unwrap().parts {
            a.push(files[&p.path]).unwrap();
        }
        assert_eq!(a.finish().unwrap(), text);
    }
}
