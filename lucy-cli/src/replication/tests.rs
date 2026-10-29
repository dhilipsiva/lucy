use super::*;

#[test]
fn targeted_retry_retains_failure_reason_honors_rate_limit_and_preserves_other_peers() {
    let temp = tempfile::tempdir().unwrap();
    let r = Replica::open(temp.path()).unwrap();
    let p = peer(&r);
    r.enroll(p.clone()).unwrap();
    save(
        &r.root.join("status.json"),
        br#"{"peers":[{"peer":"other","status":"cached"}]}"#,
    )
    .unwrap();
    drop(r);
    let fail = sync_using(temp.path(), Some("test"), false, |_, _| {
        Err("GitHub status 422 at blobs".into())
    })
    .unwrap();
    assert_eq!(fail["peers"][0]["last_error"], "GitHub status 422 at blobs");
    let deferred = sync_using(temp.path(), Some("test"), false, |_, _| {
        panic!("backoff must defer")
    })
    .unwrap();
    assert_eq!(
        deferred["peers"][0]["last_error"],
        fail["peers"][0]["last_error"]
    );
    let rate = sync_using(temp.path(), Some("test"), true, |_, _| {
        Err("GitHub rate limit; retry after 172800 (HTTP 429 at blobs)".into())
    })
    .unwrap();
    assert_eq!(rate["peers"][0]["rate_limit"], true);
    let blocked = sync_using(temp.path(), Some("test"), true, |_, _| {
        panic!("explicit retry must honor rate limit")
    })
    .unwrap();
    assert_eq!(
        blocked["peers"][0]["retry_at"],
        rate["peers"][0]["retry_at"]
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(temp.path().join("replication-v1/status.json")).unwrap())
            .unwrap();
    assert!(
        saved["peers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["peer"] == "other")
    );
    assert!(sync_using(temp.path(), None, true, |_, _| unreachable!()).is_err());
    assert!(sync_using(temp.path(), Some("missing"), false, |_, _| unreachable!()).is_err());
}
fn peer(replica: &Replica) -> Peer {
    Peer {
        name: "test".into(),
        repository: "example/memory".into(),
        scope: "public-test".into(),
        public: true,
        upload: true,
        keys: BTreeMap::from([(replica.actor(), replica.public_key())]),
        local_path: None,
        legacy_origin: None,
        source_ref: "main".into(),
        source_paths: vec![],
        credential_env: None,
        credential_file: None,
        record_origins: BTreeSet::new(),
    }
}
fn payload(text: &str) -> Value {
    json!({"record":{"origin":"test","payload":{"id":"r1","text":text,"private":false}}})
}
#[test]
fn three_replicas_converge_without_losing_concurrent_corrections() {
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let replicas: Vec<_> = dirs
        .iter()
        .map(|d| Replica::open(d.path()).unwrap())
        .collect();
    let mut peer = peer(&replicas[0]);
    for r in &replicas {
        peer.keys.insert(r.actor(), r.public_key());
    }
    replicas[0]
        .append(
            &peer,
            "r1",
            Change::Add,
            payload("exact\noriginal 🐈"),
            json!({"timestamp":"original"}),
        )
        .unwrap();
    let initial = replicas[0].read(&peer).unwrap();
    for r in &replicas[1..] {
        r.merge(&peer, initial.clone()).unwrap();
    }
    replicas[1]
        .append(&peer, "r1", Change::Correct, payload("left"), json!({}))
        .unwrap();
    replicas[2]
        .append(&peer, "r1", Change::Correct, payload("right"), json!({}))
        .unwrap();
    let left = replicas[1].read(&peer).unwrap();
    let right = replicas[2].read(&peer).unwrap();
    let mut snapshots = Vec::new();
    for (i, r) in replicas.iter().enumerate() {
        for log in if i % 2 == 0 {
            [left.clone(), right.clone(), initial.clone()]
        } else {
            [right.clone(), initial.clone(), left.clone()]
        } {
            r.merge(&peer, log).unwrap();
        }
        let log = r.read(&peer).unwrap();
        let projection = project(&log).unwrap();
        assert!(projection.active.is_empty());
        assert_eq!(projection.conflicts["r1"].len(), 2);
        assert!(projection.reasoning_error.is_some());
        snapshots.push(snapshot(&log).unwrap());
        assert_eq!(decode(&encode(&log)).unwrap(), log);
    }
    assert!(snapshots.iter().all(|s| s == &snapshots[0]));
    replicas[0]
        .append(&peer, "r1", Change::Correct, payload("resolved"), json!({}))
        .unwrap();
    let resolved = replicas[0].read(&peer).unwrap();
    assert_eq!(
        project(&resolved).unwrap().active["r1"],
        payload("resolved")
    );
    replicas[0]
        .append(&peer, "r1", Change::Retract, Value::Null, json!({}))
        .unwrap();
    replicas[0].merge(&peer, initial).unwrap();
    assert!(
        project(&replicas[0].read(&peer).unwrap())
            .unwrap()
            .deleted
            .contains("r1")
    );
    replicas[0]
        .append(&peer, "r1", Change::Restore, payload("restored"), json!({}))
        .unwrap();
    assert_eq!(
        project(&replicas[0].read(&peer).unwrap()).unwrap().active["r1"],
        payload("restored")
    );
}
#[test]
fn reject_corrupt_signatures_identity_scope_and_privacy_atomically() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let p = peer(&r);
    let op = r
        .append(&p, "r1", Change::Add, payload("first"), json!({}))
        .unwrap();
    let before = r.read(&p).unwrap();
    let mut corrupt = op.clone();
    corrupt.payload = payload("changed");
    assert!(
        r.merge(&p, BTreeMap::from([(op.id.clone(), corrupt)]))
            .is_err()
    );
    let mut other = p.clone();
    other.scope = "other".into();
    assert!(validate(&before, &other).is_err());
    other = p.clone();
    other.keys.clear();
    assert!(validate(&before, &other).is_err());
    let private = json!({"record":{"origin":"test","payload":{"id":"private","text":"secret","private":true}}});
    assert!(
        r.append(&p, "private", Change::Add, private, json!({}))
            .is_err()
    );
    assert!(
        r.append(&p, "r1", Change::Add, payload("replacement"), json!({}))
            .is_err()
    );
    assert_eq!(r.read(&p).unwrap(), before);
}
#[test]
fn inconsistent_assertions_block_projection_and_keep_evidence() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let p = peer(&r);
    r.append(
        &p,
        "one",
        Change::Add,
        json!({"assertion":"derived_only(\"prisoner\")."}),
        json!({}),
    )
    .unwrap();
    r.append(
        &p,
        "two",
        Change::Add,
        json!({"assertion":"prisoner(Zed)."}),
        json!({}),
    )
    .unwrap();
    let view = project(&r.read(&p).unwrap()).unwrap();
    assert_eq!(view.active.len(), 2);
    assert!(view.reasoning_error.is_some());
}

#[test]
fn portable_restore_preserves_tombstones_without_copying_keys_or_upload_grants() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    {
        let r = Replica::open(a.path()).unwrap();
        let p = r.local_peer();
        r.append(&p, "r", Change::Add, payload("visible"), json!({}))
            .unwrap();
        r.append(&p, "r", Change::Retract, Value::Null, json!({}))
            .unwrap();
    }
    let bundle = backup(a.path()).unwrap();
    assert_eq!(bundle["logs"][0]["peer"]["upload"], false);
    restore(b.path(), &bundle, false).unwrap();
    restore(b.path(), &bundle, true).unwrap();
    let replica = Replica::open(b.path()).unwrap();
    let p = replica.local_peer();
    assert!(
        project(&replica.read(&p).unwrap())
            .unwrap()
            .deleted
            .contains("r")
    );
    replica
        .append(&p, "r", Change::Restore, payload("restored"), json!({}))
        .unwrap();
    assert_eq!(
        project(&replica.read(&p).unwrap()).unwrap().active["r"],
        payload("restored")
    );
}
#[test]
fn malformed_legacy_archive_is_retained_but_never_queued_for_public_upload() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let p = peer(&r);
    let text = "record(\"bad\", \"private malformed bytes\", Message, Json).";
    assert!(import_source(&r, &p, "interactions.nibli", text, "c0").is_err());
    assert!(import_source(&r, &p, "interactions.nibli", text, "c0").is_err());
    assert!(r.read(&p).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(
            r.root
                .join("quarantine")
                .join(format!("{}.nibli", digest(text.as_bytes())))
        )
        .unwrap(),
        text
    );
}

#[test]
fn enrollment_cannot_export_the_unclassified_local_scope_or_broadcast_origins() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let mut p = r.local_peer();
    p.name = "remote".into();
    p.repository = "example/backup".into();
    assert!(r.enroll(p).is_err());
    let mut first = peer(&r);
    first.record_origins.insert("conversation:exact".into());
    r.enroll(first.clone()).unwrap();
    let mut second = first;
    second.name = "other".into();
    second.scope = "another-scope".into();
    assert!(r.enroll(second).is_err());
}
#[test]
fn operation_log_uses_the_actual_nibli_grammar() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let p = peer(&r);
    r.append(
        &p,
        "r1",
        Change::Add,
        payload("line\nquote \" \\ 🐈"),
        json!({}),
    )
    .unwrap();
    let text = encode(&r.read(&p).unwrap());
    assert!(nibli_kr::parse_checked(&text).is_ok());
    nibli_engine::NibliEngine::new().assert_text(&text).unwrap();
}

#[test]
fn exact_origin_routing_preserves_edits_tombstones_and_concurrency() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let mut local = r.local_peer();
    let mut destination = peer(&r);
    destination.record_origins.insert("test".into());
    r.append(&local, "r1", Change::Add, payload("original"), json!({}))
        .unwrap();
    route_local(&r, &destination).unwrap();
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = Replica::open(remote_dir.path()).unwrap();
    local.keys.insert(remote.actor(), remote.public_key());
    save(
        &r.root.join("local-keys.json"),
        &serde_json::to_vec(&local.keys).unwrap(),
    )
    .unwrap();
    remote.merge(&local, r.read(&local).unwrap()).unwrap();
    r.append(&local, "r1", Change::Correct, payload("left"), json!({}))
        .unwrap();
    remote
        .append(&local, "r1", Change::Correct, payload("right"), json!({}))
        .unwrap();
    r.merge(&local, remote.read(&local).unwrap()).unwrap();
    drop(r);
    let r = Replica::open(d.path()).unwrap();
    route_local(&r, &destination).unwrap();
    assert_eq!(
        project(&r.read(&destination).unwrap()).unwrap().conflicts["r1"].len(),
        2
    );
    r.append(&local, "r1", Change::Retract, Value::Null, json!({}))
        .unwrap();
    route_local(&r, &destination).unwrap();
    assert!(
        project(&r.read(&destination).unwrap())
            .unwrap()
            .deleted
            .contains("r1")
    );
    r.append(
        &local,
        "r1",
        Change::Restore,
        payload("restored"),
        json!({}),
    )
    .unwrap();
    route_local(&r, &destination).unwrap();
    let before = r.read(&destination).unwrap();
    assert_eq!(project(&before).unwrap().active["r1"], payload("restored"));
    route_local(&r, &destination).unwrap();
    assert_eq!(r.read(&destination).unwrap(), before);
    let mut changed = payload("relabeled");
    changed["record"]["origin"] = json!("other");
    assert!(
        r.append(&local, "r1", Change::Correct, changed, json!({}))
            .is_err()
    );
}

#[test]
fn source_revision_reverts_update_projection_without_restoring_tombstones() {
    let d = tempfile::tempdir().unwrap();
    let r = Replica::open(d.path()).unwrap();
    let p = peer(&r);
    let a = "person(Adam).";
    let b = "person(Beth).";
    for (text, revision) in [(a, "first"), (b, "second"), (a, "reverted")] {
        import_source(&r, &p, "memory.nibli", text, revision).unwrap();
        assert_eq!(
            project(&r.read(&p).unwrap()).unwrap().active["assertions:memory.nibli"]["assertion"],
            text
        );
    }
    r.append(
        &p,
        "assertions:memory.nibli",
        Change::Retract,
        Value::Null,
        json!({}),
    )
    .unwrap();
    import_source(&r, &p, "memory.nibli", b, "later").unwrap();
    assert!(
        project(&r.read(&p).unwrap())
            .unwrap()
            .deleted
            .contains("assertions:memory.nibli")
    );
}
