//! A subprocess bridge compiled only into the Rust unit-test executable.
//! No production flag, environment variable or API can select scripted replies.
use super::*;
use crate::store::{Record, Store, digest};
use serde_json::{Value, json};
use std::path::PathBuf;

#[test]
#[ignore = "invoked explicitly by the isolated engineering readiness runner"]
fn scripted_bridge() {
    let request =
        PathBuf::from(std::env::var_os("LUCY_READINESS_REQUEST").expect("private request path"));
    let response =
        PathBuf::from(std::env::var_os("LUCY_READINESS_RESPONSE").expect("private response path"));
    let value: Value = serde_json::from_slice(&std::fs::read(&request).unwrap()).unwrap();
    let args: Vec<String> = serde_json::from_value(value["args"].clone()).unwrap();
    let get = |flag: &str| -> PathBuf {
        PathBuf::from(&args[args.iter().position(|s| s == flag).unwrap() + 1])
    };
    let input = value["input"].to_string();
    let result: Result<Value> = (|| {
        if args.get(2).map(String::as_str) == Some("study-info") {
            return Ok(json!({"schema":2,"protocol":digest(PROTOCOL.as_bytes()),
                "implementation":study::implementation()?,"model":format!("scripted@{}",digest(b"mechanism-test-only")),
                "checker":digest(SPEC.as_bytes()),"policy":POLICY,"real_data":crate::store::default_root()?}));
        }
        let root = get("--root");
        if args.first().map(String::as_str) == Some("store") {
            let store = Store::open(&root)?;
            let records: Vec<Record> =
                serde_json::from_value(value["input"].clone()).map_err(|e| e.to_string())?;
            for record in &records {
                store.record(record)?;
            }
            return Ok(json!({"recorded":records.len()}));
        }
        if args.get(2).map(String::as_str) == Some("study-score") {
            return study::score_command(&root, &input);
        }
        if args.get(2).map(String::as_str) != Some("study-trial") {
            return Err("unsupported bridge operation".into());
        }
        let replies: Vec<Value> =
            serde_json::from_value(value["replies"].clone()).map_err(|e| e.to_string())?;
        study::trial_with(&root, &get("--workspace"), &input, |service| {
            service.scripted(replies)
        })
    })();
    crate::store::save(
        &response,
        &serde_json::to_vec(&match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        })
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn unrecorded_time_cannot_turn_a_late_check_or_finish_into_success() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(&tmp.path().join("memory")).unwrap();
    std::fs::write(tmp.path().join("output.txt"), "fixture").unwrap();
    let definition: Definition = serde_json::from_value(json!({"source":"fixture","session":"fixture",
        "title":"synthetic deadline","family":"verification","method":"fixture","model":"fixture@pinned",
        "environment":"fixture","budget":{"steps":12,"active_ms":100},
        "criteria":[{"id":"output","description":"exists","check":{"kind":"file_exists","path":"output.txt"}}]})).unwrap();
    let task = begin(&store, tmp.path(), definition, Observer::Native).unwrap();
    check(&store, tmp.path(), &task.id, "output").unwrap();
    let clock = runtime::ActiveClock::start(
        std::time::Instant::now() - std::time::Duration::from_millis(120),
        0,
        100,
    );
    assert!(check_with_clock(&store, tmp.path(), &task.id, "output", Some(&clock)).is_err());
    let terminal = finish_with_clock(&store, tmp.path(), &task.id, false, Some(&clock)).unwrap();
    assert!(matches!(
        terminal.event,
        Event::Finished {
            verdict: Verdict::Inconclusive,
            ..
        }
    ));
    assert!(History::read(&store).unwrap().usage(&task.id).active_ms >= 120);
}
