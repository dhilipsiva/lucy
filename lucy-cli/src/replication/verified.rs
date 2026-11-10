//! A log's verification is reused while its bytes and its peer's keys are unchanged.
//! The service reads every enrolled log for each prompt; decoding one and checking
//! every signature again costs far more than hashing its file. Each log file has at
//! most one entry, replaced when the file changes, so a process holds one decoded
//! copy of each log it reads. Nothing persists: a new process verifies each log once.
use super::{Operation, Peer, Projection, Result, decode, err, project, validate};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// How many log files one process remembers. A service has one per enrolled peer.
const CAPACITY: usize = 32;
type Key = [u8; 32];
type Entries = VecDeque<(PathBuf, Key, Arc<Verified>)>;

/// A log that passed `validate` for its peer, and its projection once someone asks.
pub(crate) struct Verified {
    pub log: BTreeMap<String, Operation>,
    projection: OnceLock<Arc<Projection>>,
}
impl Verified {
    fn new(log: BTreeMap<String, Operation>) -> Arc<Self> {
        Arc::new(Self {
            log,
            projection: OnceLock::new(),
        })
    }
    /// Projected once per content. A failure is not kept.
    pub(crate) fn projection(&self) -> Result<Arc<Projection>> {
        if let Some(found) = self.projection.get() {
            return Ok(found.clone());
        }
        let made = Arc::new(project(&self.log)?);
        Ok(self.projection.get_or_init(|| made).clone())
    }
}

/// Everything `validate` reads from a peer, then the log's exact bytes.
fn key(peer: &Peer, bytes: &[u8]) -> Key {
    let mut hash = Sha256::new();
    let mut part = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    part(b"lucy verified log v1");
    part(peer.scope.as_bytes());
    part(&[peer.public as u8]);
    part(&(peer.keys.len() as u64).to_le_bytes());
    for (actor, public) in &peer.keys {
        part(actor.as_bytes());
        part(public.as_bytes());
    }
    part(bytes);
    hash.finalize().into()
}

fn entries() -> &'static Mutex<Entries> {
    static ENTRIES: OnceLock<Mutex<Entries>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn remember(path: &Path, key: Key, verified: &Arc<Verified>) {
    // An unusable cache only costs a later read its verification.
    let Ok(mut entries) = entries().lock() else {
        return;
    };
    entries.retain(|(known, _, _)| known != path);
    if entries.len() == CAPACITY {
        entries.pop_front();
    }
    entries.push_back((path.to_owned(), key, verified.clone()));
}

/// The log at `path`, decoded and validated for `peer` unless these exact bytes
/// already were. A log that does not exist yet is empty.
pub(super) fn read(path: &Path, peer: &Peer) -> Result<Arc<Verified>> {
    if !path.exists() {
        return Ok(Verified::new(BTreeMap::new()));
    }
    let bytes = std::fs::read(path).map_err(err)?;
    let key = key(peer, &bytes);
    if let Ok(entries) = entries().lock()
        && let Some((_, _, found)) = entries
            .iter()
            .find(|(known, held, _)| known == path && *held == key)
    {
        return Ok(found.clone());
    }
    let log = decode(&String::from_utf8(bytes).map_err(err)?)?;
    validate(&log, peer)?;
    #[cfg(test)]
    VERIFICATIONS.with(|count| count.set(count.get() + 1));
    let verified = Verified::new(log);
    remember(path, key, &verified);
    Ok(verified)
}

/// Keep a log this process has just validated and written to `path` as `bytes`.
pub(super) fn wrote(
    path: &Path,
    peer: &Peer,
    bytes: &[u8],
    log: BTreeMap<String, Operation>,
) -> Arc<Verified> {
    let verified = Verified::new(log);
    remember(path, key(peer, bytes), &verified);
    verified
}

#[cfg(test)]
thread_local! {
    /// How many logs this thread has decoded and validated in full.
    pub(super) static VERIFICATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
