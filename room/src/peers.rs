//! Public archives are attributed evidence. They are never asserted into the engine.
use crate::{
    journal,
    recall::HashSpace,
    sources::{fetch_text, fetch_text_bounded},
};
use lucy_room_memory::transport::{self, Manifest, MANIFEST_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub peer: String,
    pub id: String,
    pub timestamp: String,
    pub text: String,
    pub about: Vec<String>,
    pub payload: Value,
    pub sha256: String,
}
impl Record {
    pub fn citation(&self) -> String {
        format!("[{} · {} · {}]", self.peer, self.id, self.timestamp)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub records: Vec<Record>,
    pub status: Vec<String>,
    pub space: Option<HashSpace>,
}
#[derive(Deserialize)]
struct Registry {
    version: u32,
    peers: Vec<Peer>,
}
#[derive(Deserialize)]
struct Peer {
    name: String,
    repository: String,
    public: bool,
    #[serde(default)]
    scope: String,
    #[serde(default = "main_ref")]
    source_ref: String,
    #[serde(default)]
    source_paths: Vec<String>,
}
fn main_ref() -> String {
    "main".into()
}
fn quoted(input: &str) -> Result<(String, &str), String> {
    let s = input.trim_start();
    let mut de = serde_json::Deserializer::from_str(s).into_iter::<String>();
    let value = de
        .next()
        .ok_or("missing quoted argument")?
        .map_err(|e| e.to_string())?;
    Ok((value, &s[de.byte_offset()..]))
}
pub fn parse_records(text: &str, peer: &str) -> Result<Vec<Record>, String> {
    let mut out = vec![];
    for (line_no, line) in text.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("record(") else {
            continue;
        };
        let parse = || -> Result<Record, String> {
            let (id, rest) = quoted(rest)?;
            let (payload, rest) =
                quoted(rest.trim_start().strip_prefix(',').ok_or("missing comma")?)?;
            let tail = rest.trim();
            if ![
                ", Message, Json).",
                ", Note, Json).",
                ", Summary, Json).",
                ", LegacyJournal, Json).",
                ", Claim, Json).",
                ", Decision, Json).",
            ]
            .contains(&tail)
            {
                return Err("unsupported record kind or encoding".into());
            }
            let p: Value = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
            if p.get("id").and_then(Value::as_str) != Some(&id) {
                return Err("record id disagrees with payload".into());
            }
            if p.get("private") != Some(&Value::Bool(false)) {
                return Err("record is not explicitly public".into());
            }
            if p.get("channel")
                .and_then(Value::as_str)
                .is_some_and(|s| matches!(s, "analysis" | "reasoning" | "thinking"))
            {
                return Err("non-visible record channel".into());
            }
            let text = p
                .get("text")
                .and_then(Value::as_str)
                .ok_or("record has no text")?
                .to_owned();
            let timestamp = p
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let about = p
                .get("about")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Ok(Record {
                peer: peer.into(),
                id,
                timestamp,
                text,
                about,
                sha256: lucy_room_memory::digest(payload.as_bytes()),
                payload: p,
            })
        };
        out.push(parse().map_err(|e| format!("{peer} line {}: {e}", line_no + 1))?);
    }
    Ok(out)
}
impl Memory {
    pub fn search(&self, q: &str, limit: usize) -> Vec<Record> {
        let needle = q.to_lowercase();
        let mut found: Vec<_> = self
            .records
            .iter()
            .filter(|r| r.text.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        found.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        found.truncate(limit);
        if let Some(s) = &self.space {
            for (i, _, _) in s.search(q, limit) {
                if found.len() >= limit {
                    break;
                }
                let r = &self.records[i];
                if !found
                    .iter()
                    .any(|a| a.peer == r.peer && a.id == r.id && a.sha256 == r.sha256)
                {
                    found.push(r.clone());
                }
            }
        }
        found
    }
    pub fn topics(&self) -> Vec<(String, usize)> {
        crate::recall::topic_counts(self.records.iter().map(|r| r.about.clone()))
    }
}
async fn snapshot(peer: &Peer) -> Result<Vec<Record>, String> {
    if !peer.public || !transport::repository(&peer.repository) {
        return Err("invalid public peer".into());
    }
    let reference: Value = serde_json::from_str(
        &fetch_text_bounded(
            &format!(
                "https://api.github.com/repos/{}/git/ref/heads/lucy-memory",
                peer.repository
            ),
            MANIFEST_BYTES,
        )
        .await?,
    )
    .map_err(|e| e.to_string())?;
    let head = reference["object"]["sha"]
        .as_str()
        .ok_or("memory revision missing")?;
    if reference["ref"] != "refs/heads/lucy-memory"
        || head.len() != 40
        || !head.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid memory revision".into());
    }
    let base = format!(
        "https://raw.githubusercontent.com/{}/{head}",
        peer.repository
    );
    let manifest = Manifest::parse(
        &fetch_text_bounded(&format!("{base}/manifest.json"), MANIFEST_BYTES).await?,
        &peer.repository,
        &peer.scope,
        true,
    )?;
    let mut archive = manifest.assembler("snapshot")?;
    for part in &manifest.artifact("snapshot")?.parts {
        archive.push(&fetch_text_bounded(&format!("{base}/{}", part.path), part.bytes).await?)?;
    }
    parse_snapshot(&archive.finish()?, &peer.name)
}
pub fn parse_snapshot(text: &str, peer: &str) -> Result<Vec<Record>, String> {
    let mut out = vec![];
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("record(") else {
            continue;
        };
        let (_, rest) = quoted(rest)?;
        let (payload, tail) = quoted(
            rest.trim_start()
                .strip_prefix(',')
                .ok_or("snapshot comma")?,
        )?;
        if tail.trim() != ", Snapshot, Json)." {
            return Err("unsupported snapshot encoding".into());
        }
        let v: Value = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        if let Some(p) = v.get("record").and_then(|r| r.get("payload")) {
            let id = p
                .get("id")
                .and_then(Value::as_str)
                .ok_or("snapshot record id")?;
            let line = format!(
                "record({}, {}, Message, Json).",
                serde_json::json!(id),
                serde_json::json!(p.to_string())
            );
            out.extend(parse_records(&line, peer)?);
        }
    }
    Ok(out)
}
pub async fn load(query: &str) -> Memory {
    let mut m = Memory::default();
    let params = web_sys::UrlSearchParams::new_with_str(query).ok();
    let fresh = params.as_ref().and_then(|p| p.get("fresh")).as_deref() != Some("0");
    let mut urls = vec![];
    if let Some(u) = params.as_ref().and_then(|p| p.get("registry")) {
        urls.push(u);
    } else {
        if fresh {
            urls.push("https://raw.githubusercontent.com/dhilipsiva/lucy/main/peers.json".into());
        }
        urls.push("../peers.json".into());
    }
    let mut registry = None;
    for u in urls {
        match fetch_text(&u)
            .await
            .and_then(|t| serde_json::from_str::<Registry>(&t).map_err(|e| e.to_string()))
        {
            Ok(r) if r.version == 1 => {
                m.status.push(format!("registry: {u}"));
                registry = Some(r);
                break;
            }
            Ok(_) => m.status.push("unsupported registry version".into()),
            Err(e) => m.status.push(format!("registry {u}: {e}")),
        }
    }
    let peers = registry.map(|r| r.peers).unwrap_or_else(|| {
        vec![Peer {
            name: "lucy".into(),
            repository: "dhilipsiva/lucy".into(),
            public: true,
            scope: "lucy-public".into(),
            source_ref: "main".into(),
            source_paths: vec!["lucy/interactions.nibli".into()],
        }]
    });
    for p in peers.into_iter().filter(|p| p.public) {
        let key = format!("public-archive:{}:{}", p.repository, p.source_ref);
        if fresh {
            match snapshot(&p).await {
                Ok(records) => {
                    m.status.push(format!(
                        "{}: hashed public snapshot · {} records",
                        p.name,
                        records.len()
                    ));
                    m.records.extend(records);
                    continue;
                }
                Err(e) => m.status.push(format!(
                    "{} snapshot unavailable ({e}); trying source archive",
                    p.name
                )),
            }
        }
        let mut texts = Vec::new();
        let mut failed = false;
        for path in p
            .source_paths
            .iter()
            .filter(|p| p.ends_with("interactions.nibli"))
        {
            let mut candidates = vec![];
            if fresh {
                candidates.push(format!(
                    "https://raw.githubusercontent.com/{}/{}/{}",
                    p.repository, p.source_ref, path
                ));
            }
            candidates.push(format!("../public-peers/{}/interactions.nibli", p.name));
            if p.name == "lucy" {
                candidates.push("../lucy/interactions.nibli".into());
            }
            let mut got = None;
            for url in candidates {
                if let Ok(t) = fetch_text(&url).await {
                    got = Some(t);
                    break;
                }
            }
            if let Some(t) = got {
                texts.push(t);
            } else {
                failed = true;
            }
        }
        let mut cached = false;
        let text = if !failed && !texts.is_empty() {
            let t = texts.join("\n");
            match parse_records(&t, &p.name) {
                Ok(_) => {
                    if let Err(e) = journal::cache_put(&key, &t).await {
                        m.status.push(format!("{} cache: {e}", p.name));
                    }
                    Some(t)
                }
                Err(e) => {
                    m.status.push(e);
                    cached = true;
                    journal::cache_get(&key).await.ok().flatten()
                }
            }
        } else {
            cached = true;
            journal::cache_get(&key).await.ok().flatten()
        };
        if let Some(text) = text {
            match parse_records(&text, &p.name) {
                Ok(records) => {
                    m.status.push(format!(
                        "{}: {} records ({})",
                        p.name,
                        records.len(),
                        if cached {
                            "cached; refresh unavailable"
                        } else {
                            "public source"
                        }
                    ));
                    m.records.extend(records);
                }
                Err(e) => m.status.push(e),
            }
        } else {
            m.status.push(format!("{}: unavailable", p.name));
        }
        journal::yield_now().await;
    }
    // Identical records within a peer collapse; divergent payloads and other peers survive.
    let mut seen = std::collections::BTreeSet::new();
    m.records
        .retain(|r| seen.insert((r.peer.clone(), r.id.clone(), r.sha256.clone())));
    let texts: Vec<_> = m.records.iter().map(|r| r.text.clone()).collect();
    let cached = journal::cache_get("public-hash-v2")
        .await
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str::<HashSpace>(&t).ok())
        .filter(|s| s.valid_for(&texts));
    let s = if let Some(s) = cached {
        m.status.push("hash-v2: cached".into());
        s
    } else {
        let s = HashSpace::build_yielding(&texts).await;
        if let Ok(t) = serde_json::to_string(&s) {
            let _ = journal::cache_put("public-hash-v2", &t).await;
        }
        m.status.push("hash-v2: rebuilt for current corpus".into());
        s
    };
    m.space = Some(s);
    m
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_transport_versions_feed_the_same_public_snapshot_parser() {
        let payload = serde_json::json!({"record":{"payload":{"id":"shared","text":"exact 🐦\nmessage","private":false,"channel":"user"}}});
        let text = format!(
            "record(\"target\", {}, Snapshot, Json).\n",
            serde_json::json!(payload.to_string())
        );
        for version in [1, 2] {
            let bundle = transport::bundle(serde_json::json!({"identity":"test","repository":"example/memory","scope":"test","public":true,"ref":"lucy-memory","schema":"lucy-operations-v1"}),"".into(),text.clone(),version).unwrap();
            let manifest = Manifest::parse(
                &bundle.files.last().unwrap().1,
                "example/memory",
                "test",
                true,
            )
            .unwrap();
            let mut archive = manifest.assembler("snapshot").unwrap();
            for part in &manifest.artifact("snapshot").unwrap().parts {
                archive
                    .push(
                        &bundle
                            .files
                            .iter()
                            .find(|(path, _)| path == &part.path)
                            .unwrap()
                            .1,
                    )
                    .unwrap();
            }
            let records = parse_snapshot(&archive.finish().unwrap(), "test").unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].text, "exact 🐦\nmessage");
        }
        assert!(parse_snapshot(&text.replace("false", "true"), "test").is_err());
    }
    #[test]
    fn actual_archive_is_evidence() {
        let r = parse_records(include_str!("../../lucy/interactions.nibli"), "lucy").unwrap();
        assert!(r.len() > 100);
        assert!(r.iter().any(|r| r.text.contains("Lucy")));
        assert!(r.iter().all(|r| !r.id.is_empty()));
    }
    #[test]
    fn conflicts_remain_attributed() {
        let r = parse_records(include_str!("../../lucy/interactions.nibli"), "lucy").unwrap();
        let mut m = Memory::default();
        m.records.push(r[0].clone());
        let mut other = r[0].clone();
        other.peer = "book".into();
        m.records.push(other);
        assert_eq!(m.search(&r[0].text, 5).len(), 2);
    }
}
