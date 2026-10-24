//! Durable observations and explicit acknowledgements. No inference is performed.
use crate::replication::{Replica, Result, err, now};
use crate::store::{digest, save};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
struct State {
    events: Vec<Value>,
    acknowledgements: BTreeMap<String, usize>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    store: String,
    consumer: String,
    scope: String,
    position: usize,
    seal: String,
}
fn read(replica: &Replica) -> Result<State> {
    let path = replica.root.join("continuity.json");
    if !path.exists() {
        return Ok(State::default());
    }
    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
}
fn seal(replica: &Replica, consumer: &str, scope: &str, position: usize) -> Result<String> {
    // A private per-store cursor secret is independent of the public actor key.
    let path = replica.root.join("cursor.key");
    let secret = if path.exists() {
        fs::read(path).map_err(err)?
    } else {
        let mut key = vec![0; 32];
        getrandom::fill(&mut key).map_err(err)?;
        save(&path, &key)?;
        key
    };
    let mut input = secret;
    input.extend(serde_json::to_vec(&(consumer, scope, position)).map_err(err)?);
    Ok(digest(&input))
}
/// Event ID is supplied by the producer so retrying a durable write is idempotent.
pub fn observe(
    root: &Path,
    id: &str,
    scope: &str,
    kind: &str,
    source_time: Option<&str>,
    provenance: Value,
    records: Vec<String>,
) -> Result<()> {
    let replica = Replica::open(root)?;
    let mut state = read(&replica)?;
    let event = json!({"id":id,"scope":scope,"kind":kind,"source_timestamp":source_time,
        "provenance":provenance,"verification":"service_observation","records":records});
    if let Some(old) = state.events.iter().find(|e| e["id"] == id) {
        let mut old = old.clone();
        old.as_object_mut().unwrap().remove("observed_timestamp");
        return if old == event {
            Ok(())
        } else {
            Err("continuity event identity collision".into())
        };
    }
    let mut event = event;
    event["observed_timestamp"] = json!(now());
    state.events.push(event);
    save(
        &replica.root.join("continuity.json"),
        &serde_json::to_vec(&state).map_err(err)?,
    )
}
/// The caller authorizes scope before invoking this interface. Reads do not ack.
pub fn since(root: &Path, consumer: &str, scope: &str, cursor: Option<&str>) -> Result<Value> {
    if consumer.is_empty() || scope.is_empty() {
        return Err("consumer and scope required".into());
    }
    let replica = Replica::open(root)?;
    let state = read(&replica)?;
    let key = json!([consumer, scope]).to_string();
    let position = if let Some(cursor) = cursor {
        decode(&replica, consumer, scope, cursor, state.events.len())?
    } else {
        *state.acknowledgements.get(&key).unwrap_or(&0)
    };
    let end = state.events.len().min(position + 500);
    let events: Vec<_> = state.events[position..end]
        .iter()
        .filter(|e| e["scope"] == scope)
        .collect();
    let cursor = Cursor {
        store: replica.actor(),
        consumer: consumer.into(),
        scope: scope.into(),
        position: end,
        seal: seal(&replica, consumer, scope, end)?,
    };
    Ok(
        json!({"events":events,"cursor":B64.encode(serde_json::to_vec(&cursor).map_err(err)?),"has_more":end<state.events.len()}),
    )
}
fn decode(replica: &Replica, consumer: &str, scope: &str, text: &str, len: usize) -> Result<usize> {
    let c: Cursor = serde_json::from_slice(&B64.decode(text).map_err(err)?).map_err(err)?;
    if c.store != replica.actor()
        || c.consumer != consumer
        || c.scope != scope
        || c.position > len
        || c.seal != seal(replica, consumer, scope, c.position)?
    {
        return Err("cursor does not belong to this store, consumer or scope".into());
    }
    Ok(c.position)
}
pub fn ack(root: &Path, consumer: &str, scope: &str, cursor: &str) -> Result<Value> {
    let replica = Replica::open(root)?;
    let mut state = read(&replica)?;
    let position = decode(&replica, consumer, scope, cursor, state.events.len())?;
    let current = state
        .acknowledgements
        .entry(json!([consumer, scope]).to_string())
        .or_default();
    *current = (*current).max(position);
    save(
        &replica.root.join("continuity.json"),
        &serde_json::to_vec(&state).map_err(err)?,
    )?;
    Ok(json!({"acknowledged":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_ack_restart_and_scope_isolation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        observe(
            root,
            "e1",
            "s",
            "record",
            Some("original"),
            json!({"source":"test"}),
            vec!["r1".into()],
        )
        .unwrap();
        observe(
            root,
            "e1",
            "s",
            "record",
            Some("original"),
            json!({"source":"test"}),
            vec!["r1".into()],
        )
        .unwrap();
        let a = since(root, "c", "s", None).unwrap();
        assert_eq!(a["events"].as_array().unwrap().len(), 1);
        assert_eq!(since(root, "c", "s", None).unwrap(), a);
        let c = a["cursor"].as_str().unwrap();
        assert!(ack(root, "different", "s", c).is_err());
        assert!(ack(root, "c", "other", c).is_err());
        assert!(ack(tempfile::tempdir().unwrap().path(), "c", "s", c).is_err());
        ack(root, "c", "s", c).unwrap();
        ack(root, "c", "s", c).unwrap();
        assert_eq!(since(root, "c", "s", None).unwrap()["events"], json!([]));
        assert_eq!(
            since(root, "other", "s", None).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
