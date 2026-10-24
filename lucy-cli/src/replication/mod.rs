//! Signed, scope-separated Nibli operation logs. redb is never the authority.
//! Merge is a validated set union, not a clock-based file overwrite.
use crate::store::{Record, digest, private_dir, save};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

pub mod github;
#[cfg(test)]
mod tests;
pub type Result<T> = std::result::Result<T, String>;
pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Add,
    Correct,
    Retract,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub version: u32,
    pub id: String,
    pub actor: String,
    pub counter: u64,
    pub parents: BTreeSet<String>,
    pub scope: String,
    pub target: String,
    pub change: Change,
    /// An immutable record, or {"assertion": "..."}, or retained source bytes.
    pub payload: Value,
    pub provenance: Value,
    pub signature: String,
}
impl Operation {
    fn unsigned(&self) -> Vec<u8> {
        let mut copy = self.clone();
        copy.signature.clear();
        serde_json::to_vec(&copy).expect("serializable operation")
    }
    fn verify(&self, scope: &str, keys: &BTreeMap<String, String>) -> Result<()> {
        if self.version != 1
            || self.scope != scope
            || self.counter == 0
            || self.target.is_empty()
            || self.id != format!("{}:{}", self.actor, self.counter)
            || self.parents.contains(&self.id)
        {
            return Err("invalid operation identity, version, scope, or causal reference".into());
        }
        let key: [u8; 32] = B64
            .decode(keys.get(&self.actor).ok_or("actor not enrolled")?)
            .map_err(err)?
            .try_into()
            .map_err(|_| "invalid actor key")?;
        if digest(&key) != self.actor {
            return Err("actor does not match its public key".into());
        }
        let public = VerifyingKey::from_bytes(&key).map_err(err)?;
        let signature =
            Signature::from_slice(&B64.decode(&self.signature).map_err(err)?).map_err(err)?;
        public
            .verify_strict(&self.unsigned(), &signature)
            .map_err(|_| "invalid signature".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub name: String,
    pub repository: String,
    pub scope: String,
    pub public: bool,
    pub upload: bool,
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// Only this explicitly enrolled local source may feed this peer.
    pub local_path: Option<PathBuf>,
    /// Stable legacy origin alias; moving a checkout must not rekey records.
    pub legacy_origin: Option<String>,
    pub source_ref: String,
    pub source_paths: Vec<String>,
    /// Environment variable name, never the credential itself.
    pub credential_env: Option<String>,
    #[serde(default)]
    pub credential_file: Option<PathBuf>,
    /// Exact immutable origins authorized for this upload route. No wildcard.
    #[serde(default)]
    pub record_origins: BTreeSet<String>,
}
impl Peer {
    pub fn validate(&self) -> Result<()> {
        fn safe(s: &str) -> bool {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && s != "."
                && s != ".."
        }
        if !safe(&self.name)
            || !safe(&self.scope)
            || self.repository.split('/').count() != 2
            || !self.repository.split('/').all(safe)
        {
            return Err("invalid peer name, scope or owner/repository".into());
        }
        if self.source_ref.is_empty()
            || self
                .source_paths
                .iter()
                .any(|p| p.starts_with('/') || p.split('/').any(|s| s == "..") || p.contains('\\'))
        {
            return Err("invalid source ref or path".into());
        }
        if self.public
            && self
                .source_paths
                .iter()
                .any(|p| p.to_ascii_lowercase().contains("private"))
        {
            return Err("private paths cannot be enrolled in public scopes".into());
        }
        Ok(())
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub peers: BTreeMap<String, Peer>,
    pub refresh_seconds: u64,
}

/// A short-lived local transaction. Never hold this lock while doing network I/O.
pub struct Replica {
    pub root: PathBuf,
    _lock: File,
    key: SigningKey,
    local_keys: BTreeMap<String, String>,
}
impl Replica {
    pub fn open(data: &Path) -> Result<Self> {
        let root = data.join("replication-v1");
        private_dir(&root)?;
        let path = root.join("lock");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("symlink lock".into());
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path).map_err(err)?;
        lock.lock().map_err(err)?;
        let path = root.join("actor.key");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("symlink key".into());
        }
        let bytes: [u8; 32] = if path.exists() {
            fs::read(path)
                .map_err(err)?
                .try_into()
                .map_err(|_| "bad actor key")?
        } else {
            let mut bytes = [0; 32];
            getrandom::fill(&mut bytes).map_err(err)?;
            save(&path, &bytes)?;
            bytes
        };
        let local_keys = if root.join("local-keys.json").exists() {
            serde_json::from_slice(&fs::read(root.join("local-keys.json")).map_err(err)?)
                .map_err(err)?
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            root,
            _lock: lock,
            key: SigningKey::from_bytes(&bytes),
            local_keys,
        })
    }
    pub fn actor(&self) -> String {
        digest(self.key.verifying_key().as_bytes())
    }
    pub fn local_peer(&self) -> Peer {
        let mut keys = self.local_keys.clone();
        keys.insert(self.actor(), self.public_key());
        Peer {
            name: "local".into(),
            repository: "local/local".into(),
            scope: "local-private".into(),
            public: false,
            upload: true,
            keys,
            local_path: None,
            legacy_origin: None,
            source_ref: "main".into(),
            source_paths: vec![],
            credential_env: None,
            credential_file: None,
            record_origins: BTreeSet::new(),
        }
    }
    pub fn public_key(&self) -> String {
        B64.encode(self.key.verifying_key().as_bytes())
    }
    pub fn config(&self) -> Result<Config> {
        let path = self.root.join("config.json");
        if !path.exists() {
            return Ok(Config {
                version: 1,
                refresh_seconds: 900,
                ..Config::default()
            });
        }
        let config: Config = serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
        if config.version != 1 {
            return Err("unsupported replication config".into());
        }
        Ok(config)
    }
    pub fn enroll(&self, mut peer: Peer) -> Result<()> {
        peer.validate()?;
        if peer.name == "local" || peer.scope == "local-private" {
            return Err("local private scope cannot be enrolled for transport".into());
        }
        if peer.upload {
            peer.keys.insert(self.actor(), self.public_key());
        }
        let mut config = self.config()?;
        if let Some(old) = config.peers.get(&peer.name)
            && (old.scope != peer.scope
                || old.repository != peer.repository
                || old.public != peer.public)
        {
            return Err("peer identity/scope cannot be changed; enroll a new peer".into());
        }
        if config
            .peers
            .values()
            .any(|p| p.name != peer.name && p.scope == peer.scope)
        {
            return Err("scope already routed to another peer".into());
        }
        if config
            .peers
            .values()
            .any(|p| p.name != peer.name && !p.record_origins.is_disjoint(&peer.record_origins))
        {
            return Err("record origin already routed to a different peer".into());
        }
        config.peers.insert(peer.name.clone(), peer);
        save(
            &self.root.join("config.json"),
            &serde_json::to_vec_pretty(&config).map_err(err)?,
        )
    }
    pub fn read(&self, peer: &Peer) -> Result<BTreeMap<String, Operation>> {
        peer.validate()?;
        let path = self.root.join(format!("{}.nibli", peer.scope));
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let log = decode(&fs::read_to_string(path).map_err(err)?)?;
        validate(&log, peer)?;
        Ok(log)
    }
    pub fn merge(
        &self,
        peer: &Peer,
        remote: BTreeMap<String, Operation>,
    ) -> Result<BTreeMap<String, Operation>> {
        let mut log = self.read(peer)?;
        for (id, op) in remote {
            if let Some(old) = log.get(&id)
                && old != &op
            {
                return Err("same operation ID with different bytes".into());
            }
            log.insert(id, op);
        }
        validate(&log, peer)?;
        // The authoritative log lands first. Snapshots and indexes are rebuildable.
        save(
            &self.root.join(format!("{}.nibli", peer.scope)),
            encode(&log).as_bytes(),
        )?;
        save(
            &self.root.join(format!("{}.snapshot.nibli", peer.scope)),
            snapshot(&log)?.as_bytes(),
        )?;
        Ok(log)
    }
    pub fn append(
        &self,
        peer: &Peer,
        target: &str,
        change: Change,
        payload: Value,
        provenance: Value,
    ) -> Result<Operation> {
        self.append_many(peer, vec![(target.into(), change, payload, provenance)])?
            .pop()
            .ok_or("empty append".into())
    }
    pub fn append_many(
        &self,
        peer: &Peer,
        changes: Vec<(String, Change, Value, Value)>,
    ) -> Result<Vec<Operation>> {
        if !peer.upload {
            return Err("peer is read-only".into());
        }
        let mut log = self.read(peer)?;
        let actor = self.actor();
        let mut counter = log
            .values()
            .filter(|o| o.actor == actor)
            .map(|o| o.counter)
            .max()
            .unwrap_or(0);
        let referenced: BTreeSet<_> = log
            .values()
            .flat_map(|o| o.parents.iter().cloned())
            .collect();
        let mut parents: BTreeSet<_> = log
            .keys()
            .filter(|k| !referenced.contains(*k))
            .cloned()
            .collect();
        let mut output = Vec::new();
        for (target, change, payload, provenance) in changes {
            counter += 1;
            let mut op = Operation {
                version: 1,
                id: format!("{actor}:{counter}"),
                actor: actor.clone(),
                counter,
                parents,
                scope: peer.scope.clone(),
                target,
                change,
                payload,
                provenance,
                signature: String::new(),
            };
            op.signature = B64.encode(self.key.sign(&op.unsigned()).to_bytes());
            parents = BTreeSet::from([op.id.clone()]);
            log.insert(op.id.clone(), op.clone());
            output.push(op);
        }
        self.merge(peer, log)?;
        Ok(output)
    }
}

pub fn encode(log: &BTreeMap<String, Operation>) -> String {
    log.values()
        .map(|op| {
            format!(
                "record({}, {}, Operation, Json).\n",
                json!(op.id),
                json!(serde_json::to_string(op).unwrap())
            )
        })
        .collect()
}
/// Parse two JSON-quoted Nibli constants, requiring the exact typed record tail.
pub fn decode(text: &str) -> Result<BTreeMap<String, Operation>> {
    let mut result = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
    {
        let mut tail = line
            .trim()
            .strip_prefix("record(")
            .ok_or("expected Nibli record")?;
        fn quoted(tail: &mut &str) -> Result<String> {
            let mut iter = serde_json::Deserializer::from_str(tail).into_iter::<String>();
            let value = iter.next().ok_or("missing quoted constant")?.map_err(err)?;
            *tail = &tail[iter.byte_offset()..];
            Ok(value)
        }
        let id = quoted(&mut tail)?;
        tail = tail
            .trim_start()
            .strip_prefix(',')
            .ok_or("missing separator")?
            .trim_start();
        let payload = quoted(&mut tail)?;
        if tail.trim() != ", Operation, Json)." {
            return Err("invalid Nibli operation tail".into());
        }
        let op: Operation = serde_json::from_str(&payload).map_err(err)?;
        if op.id != id {
            return Err("record ID mismatch".into());
        }
        if let Some(old) = result.insert(id, op.clone())
            && old != op
        {
            return Err("conflicting duplicate operation".into());
        }
    }
    Ok(result)
}

fn acyclic(log: &BTreeMap<String, Operation>) -> Result<()> {
    let mut done = BTreeSet::new();
    while done.len() < log.len() {
        let before = done.len();
        for (id, op) in log {
            if !done.contains(id) && op.parents.iter().all(|p| done.contains(p)) {
                done.insert(id.clone());
            }
        }
        if done.len() == before {
            return Err("missing parents or cyclic operation graph".into());
        }
    }
    Ok(())
}
fn precedes(log: &BTreeMap<String, Operation>, ancestor: &str, descendant: &str) -> bool {
    let mut todo: Vec<_> = log[descendant].parents.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if id == ancestor {
            return true;
        }
        if seen.insert(id) {
            todo.extend(log[id].parents.iter().map(String::as_str));
        }
    }
    false
}

pub fn validate(log: &BTreeMap<String, Operation>, peer: &Peer) -> Result<()> {
    acyclic(log)?;
    let mut additions: BTreeMap<&str, &Value> = BTreeMap::new();
    let mut counters: BTreeMap<(&str, u64), &str> = BTreeMap::new();
    for (id, op) in log {
        if id != &op.id {
            return Err("operation key mismatch".into());
        }
        op.verify(&peer.scope, &peer.keys)?;
        if counters.insert((&op.actor, op.counter), id).is_some() {
            return Err("actor counter reused".into());
        }
        if op.counter > 1 && !precedes(log, &format!("{}:{}", op.actor, op.counter - 1), id) {
            return Err("actor causal chain broken".into());
        }
        if op.change == Change::Add {
            if let Some(old) = additions.insert(&op.target, &op.payload)
                && old != &op.payload
            {
                return Err("immutable record identity collision".into());
            }
        } else if !log
            .values()
            .any(|p| p.target == op.target && p.change == Change::Add && precedes(log, &p.id, id))
        {
            return Err("change lacks its original record".into());
        }
        if op.change == Change::Retract && !op.payload.is_null() {
            return Err("tombstone must have null payload".into());
        }
        if op.change != Change::Retract {
            if let Some(record) = op.payload.get("record") {
                let r: Record = serde_json::from_value(record.clone()).map_err(err)?;
                r.validate()?;
                if peer.public && r.payload["private"] != false {
                    return Err("public scope requires explicit public record".into());
                }
            } else if let Some(assertion) = op.payload.get("assertion") {
                if !assertion.is_string() {
                    return Err("assertion must be text".into());
                }
            } else if op
                .payload
                .get("source_bytes")
                .and_then(Value::as_str)
                .is_none()
            {
                return Err("unsupported operation payload".into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Projection {
    pub active: BTreeMap<String, Value>,
    pub conflicts: BTreeMap<String, Vec<String>>,
    pub deleted: BTreeSet<String>,
    pub reasoning_error: Option<String>,
}
pub fn project(log: &BTreeMap<String, Operation>) -> Result<Projection> {
    acyclic(log)?;
    let mut out = Projection {
        active: BTreeMap::new(),
        conflicts: BTreeMap::new(),
        deleted: BTreeSet::new(),
        reasoning_error: None,
    };
    let targets: BTreeSet<_> = log.values().map(|o| o.target.as_str()).collect();
    for target in targets {
        let ops: Vec<_> = log.values().filter(|o| o.target == target).collect();
        let deletions: Vec<_> = ops.iter().filter(|o| o.change == Change::Retract).collect();
        let candidates: Vec<_> = ops
            .iter()
            .filter(|o| {
                o.change != Change::Retract
                    && deletions.iter().all(|d| {
                        // Only a restoration can supersede a tombstone, including a concurrent one.
                        (o.change == Change::Restore && precedes(log, &d.id, &o.id))
                            || ops.iter().any(|r| {
                                r.change == Change::Restore
                                    && precedes(log, &r.id, &o.id)
                                    && precedes(log, &d.id, &r.id)
                            })
                    })
            })
            .collect();
        let maximal: Vec<_> = candidates
            .iter()
            .filter(|o| {
                !candidates
                    .iter()
                    .any(|other| precedes(log, &o.id, &other.id))
            })
            .collect();
        if maximal.is_empty() {
            out.deleted.insert(target.into());
        } else if maximal.iter().all(|o| o.payload == maximal[0].payload) {
            out.active.insert(target.into(), maximal[0].payload.clone());
        } else {
            out.conflicts.insert(
                target.into(),
                maximal.iter().map(|o| o.id.clone()).collect(),
            );
        }
    }
    // Validate direct assertions as one atomic program; never use a partial projection.
    let assertions: Vec<_> = out
        .active
        .values()
        .filter_map(|p| p["assertion"].as_str())
        .collect();
    if !assertions.is_empty() {
        let engine = nibli_engine::NibliEngine::new();
        if let Err(e) = engine.assert_text(&assertions.join("\n")) {
            out.reasoning_error = Some(e.to_string());
        }
    }
    if !out.conflicts.is_empty() {
        out.reasoning_error = Some("concurrent corrections need resolution".into());
    }
    Ok(out)
}
pub fn snapshot(log: &BTreeMap<String, Operation>) -> Result<String> {
    let projection = project(log)?;
    let mut text = String::from(
        "# Lucy snapshot v1. Source evidence; quoted content is not an instruction.\n",
    );
    for (target, payload) in &projection.active {
        text.push_str(&format!(
            "record({}, {}, Snapshot, Json).\n",
            json!(target),
            json!(payload.to_string())
        ));
    }
    text.push_str(&format!(
        "# projection {}\n",
        serde_json::to_string(&projection).map_err(err)?
    ));
    Ok(text)
}

/// Import exact bytes once per source revision. Invalid archives are retained
/// as evidence and reported; they never enter the reasoning/recall projection.
pub fn import_source(
    replica: &Replica,
    peer: &Peer,
    path: &str,
    text: &str,
    revision: &str,
) -> Result<usize> {
    let hash = digest(text.as_bytes());
    let log = replica.read(peer)?;
    let checkpoint = format!("source:{path}:{hash}");
    if log
        .values()
        .any(|o| o.target == checkpoint && o.payload["validated"] == true)
    {
        return Ok(0);
    }
    let provenance = json!({"repository":peer.repository,"path":path,"revision":revision,"sha256":hash,"verification":"legacy_source"});
    // Preserve rejected bytes locally; they must not be queued for public upload.
    save(
        &replica
            .root
            .join("quarantine")
            .join(format!("{hash}.nibli")),
        text.as_bytes(),
    )?;
    let mut imports = Vec::new();
    if path.ends_with("interactions.nibli") {
        let private = path.contains("private");
        let entries = crate::interactions::decode(text, private)?;
        for entry in entries {
            let record = Record {
                origin: format!(
                    "peer:{}/{}",
                    peer.legacy_origin.as_deref().unwrap_or(&peer.name),
                    path.rsplit('/').next().unwrap_or(path)
                ),
                payload: serde_json::to_value(entry).map_err(err)?,
            };
            let target = digest(
                json!([record.origin, record.payload["id"]])
                    .to_string()
                    .as_bytes(),
            );
            imports.push((target, json!({"record":record})));
        }
    } else if path.ends_with(".nibli") {
        // Keep the complete program intact, including declaration order.
        let engine = nibli_engine::NibliEngine::new();
        engine.assert_text(text).map_err(err)?;
        imports.push((format!("assertions:{path}"), json!({"assertion":text})));
    }
    let count = imports.len();
    let mut changes = Vec::new();
    for (target, payload) in imports {
        let prior = log
            .values()
            .find(|o| o.target == target && o.change == Change::Add);
        let change = if let Some(prior) = prior {
            if prior.payload == payload {
                continue;
            }
            if payload.get("record").is_some() {
                return Err(format!("legacy record changed: {target}"));
            }
            Change::Correct
        } else {
            Change::Add
        };
        changes.push((target, change, payload, provenance.clone()));
    }
    changes.push((
        checkpoint,
        Change::Add,
        json!({"source_bytes":text,"validated":true}),
        provenance,
    ));
    replica.append_many(peer, changes)?;
    Ok(count)
}

pub fn sync_all(root: &Path) -> Result<Value> {
    let config = Replica::open(root)?.config()?;
    let mut reports = Vec::new();
    for peer in config.peers.values() {
        let backoff_path = root
            .join("replication-v1")
            .join(format!("{}.backoff.json", peer.scope));
        let backoff: Value = fs::read(&backoff_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let seconds = now().parse::<u64>().unwrap_or(0) / 1000;
        let result = (|| {
            if backoff["until"]
                .as_u64()
                .is_some_and(|until| until > seconds)
            {
                return Err("retry deferred by peer backoff".into());
            }
            if peer.upload {
                route_local(&Replica::open(root)?, peer)?;
            }
            if peer.upload
                && let Some(base) = &peer.local_path
            {
                for path in &peer.source_paths {
                    let text = fs::read_to_string(base.join(path)).map_err(err)?;
                    import_source(
                        &Replica::open(root)?,
                        peer,
                        path,
                        &text,
                        "local-uncommitted",
                    )?;
                }
            }
            github::sync_peer(root, peer)
        })();
        let report = match result {
            Ok(report) => {
                let _ = fs::remove_file(&backoff_path);
                report
            }
            Err(error) => {
                if error != "retry deferred by peer backoff" {
                    let hint = error
                        .strip_prefix("GitHub rate limit; retry after ")
                        .and_then(|s| s.parse::<u64>().ok());
                    let delay = hint
                        .map(|n| if n > seconds { n - seconds } else { n })
                        .unwrap_or_else(|| {
                            backoff["delay"].as_u64().unwrap_or(15).saturating_mul(2)
                        })
                        .clamp(30, 86400);
                    save(
                        &backoff_path,
                        &serde_json::to_vec(&json!({"until":seconds+delay,"delay":delay}))
                            .map_err(err)?,
                    )?;
                }
                json!({"peer":peer.name,"status":if root.join("replication-v1").join(format!("{}.nibli",peer.scope)).exists(){"cached"}else{"unavailable"},"pending":peer.upload,"error":error})
            }
        };
        let _ = crate::continuity::observe(
            root,
            &format!("sync:{}:{}", peer.name, now()),
            &peer.scope,
            "peer_sync",
            None,
            report.clone(),
            vec![],
        );
        reports.push(report);
    }
    let value = json!({"peers":reports});
    let replica = Replica::open(root)?;
    save(
        &replica.root.join("status.json"),
        &serde_json::to_vec_pretty(&value).map_err(err)?,
    )?;
    Ok(value)
}

/// Read projections separately from the legacy index so a stale native file
/// cannot resurrect a retracted record. Only explicitly enrolled scopes enter.
pub fn recall(root: &Path, query: &str, scope: Option<&str>) -> Result<Vec<Value>> {
    let replica = Replica::open(root)?;
    let mut results = Vec::new();
    let mut peers = replica.config()?.peers;
    peers.insert("local".into(), replica.local_peer());
    for peer in peers
        .values()
        .filter(|p| scope.is_none_or(|s| s == p.scope))
    {
        let mut projection = project(&replica.read(peer)?)?;
        if !replica
            .root
            .join(format!("{}.remote.nibli", peer.scope))
            .exists()
        {
            let path = replica.root.join(format!("{}.legacy.json", peer.scope));
            if path.exists() {
                let manifest: Value =
                    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
                for (path, hash) in manifest["sources"]
                    .as_object()
                    .ok_or("bad legacy manifest")?
                {
                    let hash = hash.as_str().ok_or("bad legacy digest")?;
                    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err("bad legacy digest".into());
                    }
                    let text = fs::read_to_string(
                        replica.root.join("legacy").join(format!("{hash}.nibli")),
                    )
                    .map_err(err)?;
                    if digest(text.as_bytes()) != hash {
                        return Err("legacy source hash mismatch".into());
                    }
                    if path.ends_with("interactions.nibli") {
                        for record in crate::interactions::decode(&text, path.contains("private"))?
                        {
                            projection.active.insert(record.id.clone(),json!({"record":{"origin":format!("peer:{}/{}",peer.legacy_origin.as_deref().unwrap_or(&peer.name),path.rsplit('/').next().unwrap_or(path)),"payload":record},"revision":manifest["revision"],"verification":"legacy_source"}));
                        }
                    } else {
                        projection.active.insert(path.clone(),json!({"assertion":text,"verification":"legacy_source","revision":manifest["revision"]}));
                    }
                }
            }
        }
        for (target, payload) in projection.active {
            if payload.get("source_bytes").is_some() {
                continue;
            }
            let text = payload["record"]["payload"]["text"]
                .as_str()
                .or_else(|| payload["assertion"].as_str())
                .unwrap_or("");
            let score = query
                .split_whitespace()
                .filter(|w| text.to_lowercase().contains(&w.to_lowercase()))
                .count();
            if score > 0 {
                results.push(json!({"scope":peer.scope,"peer":peer.name,"target":target,"origin":payload["record"]["origin"],"id":payload["record"]["payload"]["id"],"excerpt":text,"payload":payload,"score":score,"reasoning_blocked":projection.reasoning_error}));
            }
        }
    }
    results.sort_by_key(|r| std::cmp::Reverse(r["score"].as_u64().unwrap_or(0)));
    results.truncate(12);
    Ok(results)
}

pub fn command(command: &str, args: &[String], stdin: &str) -> Result<Value> {
    let arg = |name: &str| {
        args.windows(2)
            .find(|a| a[0] == name)
            .map(|a| a[1].as_str())
    };
    let root = arg("--root")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(crate::store::default_root)?;
    match command {
        "sync" => sync_all(&root),
        "peer" if args.first().is_some_and(|a| a == "discover") => github::discover(&root),
        "peer" => {
            let replica = Replica::open(&root)?;
            match args.first().map(String::as_str) {
                Some("migrate") => {
                    let peer = replica.local_peer();
                    let existing = replica.read(&peer)?;
                    let mut changes = Vec::new();
                    let mut paths = fs::read_dir(root.join("records"))
                        .map_err(err)?
                        .map(|e| e.map(|e| e.path()).map_err(err))
                        .collect::<Result<Vec<_>>>()?;
                    paths.sort();
                    for path in paths
                        .into_iter()
                        .filter(|p| p.extension().is_some_and(|e| e == "json"))
                    {
                        let bytes = fs::read(&path).map_err(err)?;
                        let record: Record = serde_json::from_slice(&bytes).map_err(err)?;
                        record.validate()?;
                        if record.origin.starts_with("peer:") {
                            continue;
                        }
                        let target = digest(
                            json!([record.origin, record.payload["id"]])
                                .to_string()
                                .as_bytes(),
                        );
                        let payload = json!({"record":record});
                        if let Some(old) = existing
                            .values()
                            .find(|o| o.target == target && o.change == Change::Add)
                        {
                            if old.payload != payload {
                                return Err("legacy identity collision".into());
                            }
                            continue;
                        }
                        changes.push((target,Change::Add,payload,json!({"legacy_sha256":digest(&bytes),"verification":"local_legacy_import"})));
                    }
                    let count = changes.len();
                    replica.append_many(&peer, changes)?;
                    Ok(json!({"migrated":count,"scope":"local-private","originals_retained":true}))
                }
                Some("enroll") => {
                    let peer: Peer = serde_json::from_str(stdin).map_err(err)?;
                    replica.enroll(peer)?;
                    Ok(json!({"enrolled":true}))
                }
                Some("list") => Ok(
                    json!({"peers":replica.config()?.peers,"actor":replica.actor(),"public_key":replica.public_key()}),
                ),
                Some("change") => {
                    let request: Value = serde_json::from_str(stdin).map_err(err)?;
                    let mut config = replica.config()?;
                    config.peers.insert("local".into(), replica.local_peer());
                    let peer = config
                        .peers
                        .get(request["peer"].as_str().ok_or("peer required")?)
                        .ok_or("peer not enrolled")?;
                    let op = replica.append(
                        peer,
                        request["target"].as_str().ok_or("target required")?,
                        serde_json::from_value(request["change"].clone()).map_err(err)?,
                        request["payload"].clone(),
                        request["provenance"].clone(),
                    )?;
                    Ok(json!({"operation":op.id}))
                }
                _ => Err("use peer enroll --stdin, list, or change --stdin".into()),
            }
        }
        "doctor" => {
            let replica = Replica::open(&root)?;
            let config = replica.config()?;
            let mut checks = Vec::new();
            for peer in config.peers.values() {
                let result = replica.read(peer).and_then(|log| project(&log));
                checks.push(json!({"peer":peer.name,"valid":result.is_ok(),"error":result.as_ref().err(),"projection":result.ok()}));
            }
            #[cfg(unix)]
            let service = crate::desktop::probe();
            #[cfg(not(unix))]
            let service = json!({"status":"unsupported","reason":"CLI-only on this platform"});
            Ok(
                json!({"version":env!("CARGO_PKG_VERSION"),"data":root,"actor":replica.actor(),"peers":checks,"service":service,"network_tested":false,"model_invoked":false}),
            )
        }
        "continuity" => {
            let consumer = arg("--consumer").ok_or("--consumer required")?;
            let scope = arg("--scope").ok_or("--scope required")?;
            match args.first().map(String::as_str) {
                Some("since") => crate::continuity::since(&root, consumer, scope, arg("--cursor")),
                Some("ack") => crate::continuity::ack(
                    &root,
                    consumer,
                    scope,
                    arg("--cursor").ok_or("--cursor required")?,
                ),
                _ => Err("use continuity since or ack".into()),
            }
        }
        _ => Err("unknown replication command".into()),
    }
}

/// Once an origin has a canonical log, suppress legacy-index copies of its
/// targets (including tombstones/conflicts). Retrieval then uses the projection.
pub fn filter_legacy(root: &Path, memories: &mut Vec<Value>) -> Result<()> {
    let replica = Replica::open(root)?;
    let mut origins = BTreeSet::new();
    let mut peers = replica.config()?.peers;
    peers.insert("local".into(), replica.local_peer());
    for peer in peers.values() {
        for op in replica.read(peer)?.values() {
            if let Some(origin) = op.payload["record"]["origin"].as_str() {
                origins.insert((
                    origin.to_string(),
                    op.payload["record"]["payload"]["id"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                ));
            }
        }
    }
    memories.retain(|r| {
        !origins.contains(&(
            r["origin"].as_str().unwrap_or("").to_string(),
            r["id"].as_str().unwrap_or("").to_string(),
        ))
    });
    Ok(())
}

/// Commit an original to the Nibli log before publishing it in the legacy index.
/// Historical JSON copies remain for compatibility and private backup replay.
pub fn record_local(root: &Path, record: &Record) -> Result<()> {
    // Native peers keep their own authoritative files/logs and migration aliases.
    if record.origin.starts_with("peer:") {
        return Ok(());
    }
    let replica = Replica::open(root)?;
    let peer = replica.local_peer();
    let target = digest(
        json!([record.origin, record.payload["id"]])
            .to_string()
            .as_bytes(),
    );
    let log = replica.read(&peer)?;
    let payload = json!({"record":record});
    if let Some(old) = log
        .values()
        .find(|o| o.target == target && o.change == Change::Add)
    {
        return if old.payload == payload {
            Ok(())
        } else {
            Err("immutable local identity collision".into())
        };
    }
    replica.append(
        &peer,
        &target,
        Change::Add,
        payload,
        json!({"verification":"local_original"}),
    )?;
    Ok(())
}
fn route_local(replica: &Replica, peer: &Peer) -> Result<()> {
    if peer.record_origins.is_empty() {
        return Ok(());
    }
    let source = replica.read(&replica.local_peer())?;
    let current = replica.read(peer)?;
    let mut changes = Vec::new();
    for op in source.values().filter(|o| o.change == Change::Add) {
        let Some(origin) = op.payload["record"]["origin"].as_str() else {
            continue;
        };
        if !peer.record_origins.contains(origin) || current.values().any(|o| o.target == op.target)
        {
            continue;
        }
        changes.push((
            op.target.clone(),
            Change::Add,
            op.payload.clone(),
            json!({"local_operation":op.id,"provenance":op.provenance}),
        ));
    }
    if !changes.is_empty() {
        replica.append_many(peer, changes)?;
    }
    Ok(())
}

/// Portable backup of canonical logs, with public verification keys and no
/// credentials, signing keys, local paths, upload grants or activation state.
pub fn backup(root: &Path) -> Result<Value> {
    let replica = Replica::open(root)?;
    let mut peers = replica.config()?.peers;
    peers.insert("local".into(), replica.local_peer());
    let mut logs = Vec::new();
    for mut peer in peers.into_values() {
        let operations = encode(&replica.read(&peer)?);
        peer.upload = false;
        peer.local_path = None;
        peer.credential_env = None;
        peer.credential_file = None;
        peer.record_origins.clear();
        logs.push(
            json!({"peer":peer,"operations":operations,"sha256":digest(operations.as_bytes())}),
        );
    }
    Ok(json!({"version":1,"logs":logs}))
}
/// Validate the entire bundle before writing any canonical log. Restoring a
/// private backup is explicit; it never restores automatic upload permission.
pub fn restore(root: &Path, bundle: &Value, apply: bool) -> Result<()> {
    if bundle["version"] != 1 {
        return Err("unsupported operation backup".into());
    }
    let replica = Replica::open(root)?;
    let config = replica.config()?;
    let mut ready = Vec::new();
    for entry in bundle["logs"].as_array().ok_or("missing operation logs")? {
        let mut peer: Peer = serde_json::from_value(entry["peer"].clone()).map_err(err)?;
        peer.validate()?;
        let text = entry["operations"]
            .as_str()
            .ok_or("missing operation text")?;
        if entry["sha256"] != digest(text.as_bytes()) {
            return Err("operation backup hash mismatch".into());
        }
        let remote = decode(text)?;
        validate(&remote, &peer)?;
        if peer.name == "local" {
            if peer.scope != "local-private" || peer.public || peer.repository != "local/local" {
                return Err("invalid local backup scope".into());
            }
            peer.keys.extend(replica.local_peer().keys);
        } else if let Some(existing) = config.peers.get(&peer.name) {
            if peer.scope != existing.scope
                || peer.repository != existing.repository
                || peer.public != existing.public
            {
                return Err("backup peer identity mismatch".into());
            }
            peer.keys.extend(existing.keys.clone());
        }
        peer.upload = false;
        peer.local_path = None;
        peer.credential_file = None;
        peer.credential_env = None;
        peer.record_origins.clear();
        let mut merged = replica.read(&peer)?;
        for (id, op) in remote {
            if merged.get(&id).is_some_and(|old| old != &op) {
                return Err("backup operation collision".into());
            }
            merged.insert(id, op);
        }
        validate(&merged, &peer)?;
        ready.push((peer, merged));
    }
    if apply {
        for (peer, log) in ready {
            if peer.name == "local" {
                save(
                    &replica.root.join("local-keys.json"),
                    &serde_json::to_vec(&peer.keys).map_err(err)?,
                )?;
            } else if !config.peers.contains_key(&peer.name) {
                replica.enroll(peer.clone())?;
            } else {
                let mut existing = config.peers[&peer.name].clone();
                existing.keys = peer.keys.clone();
                replica.enroll(existing)?;
            }
            replica.merge(&peer, log)?;
        }
    }
    Ok(())
}

pub fn local_originals(root: &Path) -> Result<Vec<Record>> {
    let replica = Replica::open(root)?;
    let mut records = BTreeMap::new();
    for op in replica
        .read(&replica.local_peer())?
        .into_values()
        .filter(|o| o.change == Change::Add)
    {
        if let Some(record) = op.payload.get("record") {
            let record: Record = serde_json::from_value(record.clone()).map_err(err)?;
            if !record.origin.starts_with("peer:") {
                records.insert(op.target, record);
            }
        }
    }
    Ok(records.into_values().collect())
}
