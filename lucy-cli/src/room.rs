//! Portable private room archives. Preview is read-only; commit targets exactly one private peer.
use crate::{
    replication::{self, Change, Operation, Peer},
    store::{self, Record, Store},
};
use lucy_room_memory::RoomArchiveV1;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn private_target(peer: &Peer) -> Result<()> {
    peer.validate()?;
    if peer.public || !peer.upload || peer.name == "local" {
        return Err("room archives require an enrolled, writable private peer".into());
    }
    Ok(())
}
type Changes = Vec<(String, Change, Value, Value)>;
pub(crate) fn changes(
    log: &BTreeMap<String, Operation>,
    records: &[Record],
    archive: &[u8],
) -> Result<Changes> {
    let mut out = vec![];
    let mut seen = BTreeSet::new();
    for r in records {
        let target = r.key();
        if !seen.insert(target.clone()) {
            continue;
        }
        let payload = json!({"record":r});
        let prior: Vec<_> = log
            .values()
            .filter(|o| o.target == target && o.change == Change::Add)
            .collect();
        if prior.iter().any(|o| o.payload != payload) {
            return Err(format!(
                "immutable room identity conflict: {}",
                r.payload["id"]
            ));
        }
        if !prior.is_empty() {
            continue;
        }
        out.push((target,Change::Add,payload,json!({"verification":"room_export","archive_sha256":store::digest(archive),"source":r.payload["source"],"room":r.payload["provenance"]["room"]})));
    }
    Ok(out)
}
fn peer(root: &Path, name: &str) -> Result<Peer> {
    let path = root.join("replication-v1/config.json");
    let config: replication::Config = serde_json::from_slice(
        &fs::read(path).map_err(|_| "No peer configuration; enroll a private peer first")?,
    )
    .map_err(err)?;
    if config.version != 1 {
        return Err("unsupported peer configuration".into());
    }
    let peer = config
        .peers
        .get(name)
        .ok_or_else(|| format!("peer {name} is not enrolled"))?
        .clone();
    private_target(&peer)?;
    Ok(peer)
}
fn records(archive: &RoomArchiveV1) -> Result<Vec<Record>> {
    let mut seen = BTreeSet::new();
    archive
        .messages
        .iter()
        .filter(|m| seen.insert(m.key()))
        .map(|m| {
            let r = Record {
                origin: m.origin(),
                payload: serde_json::to_value(m).map_err(err)?,
            };
            r.validate()?;
            Ok(r)
        })
        .collect()
}
/// No Store/Replica open here: those constructors create locks and indexes.
pub fn preview(root: &Path, bytes: &[u8], name: &str) -> Result<Value> {
    let archive = RoomArchiveV1::parse(bytes)?;
    let records = records(&archive)?;
    let peer = peer(root, name)?;
    let log_path = root
        .join("replication-v1")
        .join(format!("{}.nibli", peer.scope));
    let log = if log_path.exists() {
        let log = replication::decode(&fs::read_to_string(log_path).map_err(err)?)?;
        replication::validate(&log, &peer)?;
        log
    } else {
        BTreeMap::new()
    };
    let additions = changes(&log, &records, bytes)?.len();
    let mut duplicates = 0;
    for r in &records {
        let path = root.join("records").join(format!("{}.json", r.key()));
        if path.exists() {
            let old: Record = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
            if old != *r {
                return Err(format!(
                    "room record ID reused with different content: {}",
                    r.payload["id"]
                ));
            }
            duplicates += 1;
        }
    }
    let sessions: BTreeSet<_> = archive
        .messages
        .iter()
        .map(|m| (&m.source, &m.session, &m.provenance.room))
        .collect();
    Ok(
        json!({"mode":"preview","archive_sha256":store::digest(bytes),"messages":records.len(),"already_local":duplicates,"new_peer_records":additions,"peer":name,"repository":peer.repository,"scope":peer.scope,"private":true,"branch":"lucy-memory","origins":sessions,"writes":false}),
    )
}
pub fn import_local(root: &Path, bytes: &[u8], name: &str) -> Result<Value> {
    let mut report = preview(root, bytes, name)?;
    let archive = RoomArchiveV1::parse(bytes)?;
    let records = records(&archive)?;
    let store = Store::open(root)?;
    let added = store.import_room(&records, name, bytes)?;
    drop(store);
    report["mode"] = json!("imported");
    report["writes"] = json!(true);
    report["imported"] = json!(added);
    report["pending"] = json!(true);
    report["original_archive"] = json!(
        root.join("room-imports")
            .join(format!("{}.json", store::digest(bytes)))
    );
    Ok(report)
}
pub fn command(args: &[String]) -> Result<Value> {
    if args.first().map(String::as_str) != Some("import") {
        return Err("usage: lucy room import FILE --peer personal [--commit] [--root DIR]".into());
    }
    let file = PathBuf::from(args.get(1).ok_or("room import requires an archive file")?);
    let mut root = None;
    let mut name = "personal".to_string();
    let mut commit = false;
    let mut iter = args.iter().skip(2);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--commit" => commit = true,
            "--peer" => name = iter.next().ok_or("missing --peer value")?.clone(),
            "--root" => root = Some(PathBuf::from(iter.next().ok_or("missing --root value")?)),
            _ => return Err(format!("unknown room option {flag}")),
        }
    }
    let root = root.map(Ok).unwrap_or_else(store::default_root)?;
    let bytes = fs::read(&file).map_err(err)?;
    if !commit {
        return preview(&root, &bytes, &name);
    }
    let mut report = import_local(&root, &bytes, &name)?;
    let peer = peer(&root, &name)?;
    match replication::github::sync_private_peer(&root, &peer) {
        Ok(receipt) => {
            report["pending"] = receipt["pending"].clone();
            report["commit"] = receipt["head"].clone();
            report["sync"] = receipt;
        }
        Err(error) => {
            report["pending"] = json!(true);
            report["error"] = json!(error);
            report["retry"] = json!(
                "Repeat this import with --commit; originals and the signed outbox remain private."
            );
        }
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    use lucy_room_memory::{Message, Provenance};
    fn setup() -> (tempfile::TempDir, Peer) {
        let dir = tempfile::tempdir().unwrap();
        let rep = replication::Replica::open(dir.path()).unwrap();
        let mut p = rep.local_peer();
        p.name = "personal".into();
        p.scope = "personal-private".into();
        p.repository = "example/private-room".into();
        rep.enroll(p).unwrap();
        let p = rep.config().unwrap().peers["personal"].clone();
        drop(rep);
        (dir, p)
    }
    fn archive() -> RoomArchiveV1 {
        RoomArchiveV1 {
            messages: vec![Message {
                id: "unique-1".into(),
                source: "lucy-room".into(),
                session: "visit-1".into(),
                role: "user".into(),
                channel: "user".into(),
                speaker: "visitor".into(),
                text: "தமிழ் 🐦\nOriginal \\ question".into(),
                timestamp: "2026-10-26T00:00:00Z".into(),
                timestamp_kind: "capture".into(),
                private: true,
                provenance: Provenance {
                    room: "https://example.test/lucy/room/".into(),
                    device: "device-1".into(),
                    status: "complete".into(),
                    ..Default::default()
                },
            }],
            ..Default::default()
        }
    }
    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        for entry in fs::read_dir(root).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                out.extend(files(&p));
            } else {
                out.insert(p.clone(), fs::read(p).unwrap());
            }
        }
        out
    }
    #[test]
    fn preview_writes_nothing_import_is_exact_idempotent_and_conflicts_are_atomic() {
        let (dir, p) = setup();
        let a = archive();
        let bytes = a.json().unwrap().into_bytes();
        let before = files(dir.path());
        assert_eq!(
            preview(dir.path(), &bytes, "personal").unwrap()["writes"],
            false
        );
        assert_eq!(files(dir.path()), before);
        assert_eq!(
            import_local(dir.path(), &bytes, "personal").unwrap()["imported"],
            1
        );
        let imported = files(dir.path());
        assert_eq!(
            import_local(dir.path(), &bytes, "personal").unwrap()["imported"],
            0
        );
        let rep = replication::Replica::open(dir.path()).unwrap();
        let log = rep.read(&p).unwrap();
        assert_eq!(log.len(), 1);
        let record = &log.values().next().unwrap().payload["record"];
        assert_eq!(record["origin"], a.messages[0].origin());
        assert_eq!(
            record["payload"],
            serde_json::to_value(&a.messages[0]).unwrap()
        );
        assert!(
            rep.config().unwrap().peers["personal"]
                .record_origins
                .is_empty()
        );
        drop(rep);
        let before = files(dir.path());
        let mut bad = a.clone();
        bad.messages[0].text = "changed".into();
        let mut new = a.messages[0].clone();
        new.id = "new-2".into();
        bad.messages.insert(0, new);
        assert!(import_local(dir.path(), bad.json().unwrap().as_bytes(), "personal").is_err());
        assert_eq!(files(dir.path()), before);
        assert!(
            imported
                .keys()
                .any(|p| p.to_string_lossy().contains("room-imports"))
        );
        let results =
            replication::recall(dir.path(), "Original", Some("personal-private")).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|v| v.to_string().contains("lucy-room")));
    }
    #[test]
    fn public_and_hidden_messages_are_rejected_before_writes() {
        let (dir, mut p) = setup();
        p.name = "public".into();
        p.scope = "public".into();
        p.repository = "example/public".into();
        p.public = true;
        let rep = replication::Replica::open(dir.path()).unwrap();
        rep.enroll(p).unwrap();
        drop(rep);
        let a = archive();
        let before = files(dir.path());
        assert!(import_local(dir.path(), a.json().unwrap().as_bytes(), "public").is_err());
        let mut v = serde_json::to_value(&a).unwrap();
        v["messages"][0]["channel"] = json!("analysis");
        assert!(import_local(dir.path(), v.to_string().as_bytes(), "personal").is_err());
        assert_eq!(files(dir.path()), before);
    }
}
