use crate::replication::{Change, Peer, Replica};
use crate::store::{Record, Retrieval, Store, command, rank, retrieve};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Every file under a backup destination, as its relative path and bytes.
fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy();
                found.push((name.into_owned(), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut found = Vec::new();
    if root.exists() {
        walk(root, root, &mut found);
    }
    found.sort();
    found
}

/// Whether a marker appears in any file name or any file's bytes.
fn holds(files: &[(String, Vec<u8>)], marker: &str) -> bool {
    files.iter().any(|(name, bytes)| {
        name.contains(marker) || bytes.windows(marker.len()).any(|w| w == marker.as_bytes())
    })
}

fn manifest_of(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The part paths one log's descriptor names, in order.
fn parts_of(manifest: &Path, log: usize) -> Vec<String> {
    manifest_of(manifest)["replication"]["logs"][log]["archive"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["path"].as_str().unwrap().to_owned())
        .collect()
}

/// A registered folder whose memory file carries one marker.
fn folder(store: &Store, base: &Path, name: &str, marker: &str) {
    let home = base.join(name).join("lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    std::fs::write(home.join("memory.nibli"), format!("bird({marker}).\n")).unwrap();
    store.register(name, &home).unwrap();
}

/// An enrolled peer whose log holds one record and one retained source, both marked.
fn enrolled(root: &Path, name: &str, repository: &str, public: bool, marker: &str) {
    let replica = Replica::open(root).unwrap();
    replica
        .enroll(Peer {
            name: name.into(),
            repository: repository.into(),
            scope: format!("{name}-scope"),
            public,
            upload: true,
            keys: Default::default(),
            local_path: Some(format!("/nowhere/{name}-checkout").into()),
            legacy_origin: Some(name.into()),
            source_ref: "main".into(),
            source_paths: vec![],
            credential_env: None,
            credential_file: Some(format!("/nowhere/{name}-token").into()),
            record_origins: Default::default(),
        })
        .unwrap();
    let peer = replica.config().unwrap().peers[name].clone();
    let record = json!({"record":{"origin":format!("peer:{name}/interactions.nibli"),
        "payload":{"id":"r1","private":!public,"text":format!("a record about {marker}")}}});
    replica
        .append(&peer, "r1", Change::Add, record, json!({}))
        .unwrap();
    let source = json!({"source_bytes":format!("bytes about {marker}"),"validated":true});
    replica
        .append(&peer, "source:memory", Change::Add, source, json!({}))
        .unwrap();
}

fn record(id: &str, text: &str) -> Record {
    Record {
        origin: "conversation:test/one".into(),
        payload: json!({
            "id":id, "source":"test","session":"one","role":"user","channel":"user",
            "timestamp":"2026-10-20T01:02:03Z", "private":true, "text":text
        }),
    }
}

#[test]
fn lossless_write_reopen_repair_and_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let original = record("first", "café 😀\n  exact\t\r\n");
    {
        let store = Store::open(dir.path()).unwrap();
        store.record(&original).unwrap();
        store.record(&original).unwrap();
        assert!(store.record(&record("first", "changed")).is_err());
        assert_eq!(store.all().unwrap(), vec![original.clone()]);
    }
    std::fs::remove_file(dir.path().join("memory.redb")).unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.sync().unwrap();
    assert_eq!(store.all().unwrap(), vec![original]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("memory.redb"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn peers_are_scoped_refreshable_and_do_not_assert_retrieved_text() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("peer/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    std::fs::write(home.join("memory.nibli"), "bird(Enma).\n").unwrap();
    std::fs::write(dir.path().join("peer/fixture.nibli"), "fixture(Secret).\n").unwrap();
    let store = Store::open(&dir.path().join("shared")).unwrap();
    store.register("nibli", &home).unwrap();
    store.sync().unwrap();
    assert_eq!(store.search("Enma", "", 8, None).unwrap().len(), 1);
    assert!(store.search("Secret", "", 8, None).unwrap().is_empty());
    std::fs::write(home.join("memory.nibli"), "bird(Pity).\n").unwrap();
    store.sync().unwrap();
    assert!(store.search("Enma", "", 8, None).unwrap().is_empty());
    assert_eq!(store.search("Pity", "", 8, None).unwrap().len(), 1);
    std::fs::remove_file(home.join("memory.nibli")).unwrap();
    store.sync().unwrap();
    assert!(store.search("Pity", "", 8, None).unwrap().is_empty());
}

#[test]
fn backup_restores_exactly_on_new_host_and_detects_corruption_before_writes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("host-a")).unwrap();
    store
        .record(&record("first", "original whitespace\n  kept"))
        .unwrap();
    store
        .record(&record("second", "another visible message"))
        .unwrap();
    let backup = dir.path().join("backup");
    let manifest = store.backup(&backup).unwrap();
    let restored = Store::open(&dir.path().join("host-b")).unwrap();
    restored.restore(&manifest).unwrap();
    assert_eq!(restored.all().unwrap(), store.all().unwrap());
    restored.restore(&manifest).unwrap();
    assert_eq!(restored.all().unwrap().len(), 2);
    let object = std::fs::read_dir(backup.join("objects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(object, b"corrupted").unwrap();
    let empty = Store::open(&dir.path().join("host-c")).unwrap();
    assert!(empty.restore(&manifest).is_err());
    assert!(empty.all().unwrap().is_empty());
}

#[test]
fn record_rejects_hidden_channels_and_source_id_conflicts_across_restore() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("a")).unwrap();
    let mut hidden = record("x", "fixture");
    hidden.payload["channel"] = json!("analysis");
    assert!(store.record(&hidden).is_err());
    store.record(&record("x", "first")).unwrap();
    let manifest = store.backup(&dir.path().join("backup")).unwrap();
    let other = Store::open(&dir.path().join("b")).unwrap();
    other.record(&record("x", "conflicting")).unwrap();
    assert!(other.restore(&manifest).is_err());
    assert_eq!(other.all().unwrap(), vec![record("x", "conflicting")]);
}

#[test]
fn native_file_write_is_immediately_shared() {
    let dir = tempfile::tempdir().unwrap();
    let mut env = crate::tests::env_in(dir.path());
    env.store = Some(dir.path().join("shared"));
    let args = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(crate::run(&args(&["init"]), "", Some(env.clone())).code, 0);
    let result = crate::run(
        &args(&[
            "record",
            "a visible native message",
            "--speaker",
            "User",
            "--source",
            "cli",
            "--session",
            "native",
        ]),
        "",
        Some(env.clone()),
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let store = Store::open(env.store.as_ref().unwrap()).unwrap();
    assert_eq!(
        store.search("visible native", "", 8, None).unwrap().len(),
        1
    );
}

#[test]
fn multiple_processes_share_the_same_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut threads = Vec::new();
    for i in 0..5 {
        let root = dir.path().to_owned();
        threads.push(std::thread::spawn(move || {
            let store = Store::open(&root).unwrap();
            store
                .record(&record(&i.to_string(), "parallel visible record"))
                .unwrap();
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(Store::open(dir.path()).unwrap().all().unwrap().len(), 5);
}

#[test]
fn search_stdin_keeps_query_out_of_process_arguments() {
    let dir = tempfile::tempdir().unwrap();
    Store::open(dir.path())
        .unwrap()
        .record(&record("a", "unicorn library"))
        .unwrap();
    let args = ["search", "--root", dir.path().to_str().unwrap(), "--stdin"].map(str::to_owned);
    let result = command(&args, r#"{"query":"unicorn"}"#).unwrap();
    assert_eq!(result["results"][0]["id"], "a");
    assert_eq!(result["mode"], "lexical");
}

/// A local embedding model. While `awake` it lists one model, says whether that model is
/// `loaded`, and embeds anything as the same vector, noting how many characters it was
/// given; otherwise it accepts a request and never answers it.
fn model(awake: Arc<AtomicBool>, loaded: Arc<AtomicBool>, given: Arc<Mutex<Vec<usize>>>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        let mut unanswered = Vec::new();
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            if !awake.load(Ordering::SeqCst) {
                unanswered.push(stream);
                continue;
            }
            let mut raw = Vec::new();
            let mut buffer = [0u8; 4096];
            let body_at = loop {
                let read = stream.read(&mut buffer).unwrap();
                raw.extend_from_slice(&buffer[..read]);
                if let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
                    let length: usize = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .map_or(0, |value| value.trim().parse().unwrap());
                    if read == 0 || raw.len() >= split + 4 + length {
                        break split + 4;
                    }
                } else if read == 0 {
                    break raw.len();
                }
            };
            let body = if raw.starts_with(b"GET /api/tags") {
                r#"{"models":[{"name":"test:latest","digest":"d1"}]}"#
            } else if raw.starts_with(b"GET /api/ps") {
                if loaded.load(Ordering::SeqCst) {
                    r#"{"models":[{"name":"test:latest","digest":"d1"}]}"#
                } else {
                    r#"{"models":[]}"#
                }
            } else {
                let sent: Value = serde_json::from_slice(&raw[body_at..]).unwrap();
                let input = sent["input"].as_str().unwrap();
                given.lock().unwrap().push(input.chars().count());
                r#"{"embeddings":[[0.6,0.8]]}"#
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    url
}

#[test]
fn retrieval_can_skip_the_index_refresh_and_never_waits_long_for_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    let root = dir.path();
    let store = Store::open(root).unwrap();
    store
        .record(&record("a", "the heron stands in the reeds"))
        .unwrap();
    folder(&store, base.path(), "marsh", "bittern");
    let quick = |query, sync| Retrieval {
        query,
        exclude: "",
        limit: 8,
        sync,
        embed_within: Duration::from_millis(300),
        embed_chars: 3_000,
        embed_loaded_only: false,
    };
    let count = |found: &Value| found["results"].as_array().unwrap().len();
    // A search told not to refresh the index does not read the registered folder.
    let found = retrieve(store, &quick("bittern", false)).unwrap();
    assert_eq!(count(&found), 0);
    assert!(found["sync"].is_null());
    let found = retrieve(Store::open(root).unwrap(), &quick("bittern", true)).unwrap();
    assert_eq!(count(&found), 1);
    assert_eq!(found["sync"]["peers"], json!(["marsh"]));
    let found = retrieve(Store::open(root).unwrap(), &quick("bittern", false)).unwrap();
    assert_eq!(count(&found), 1);
    assert_eq!(found["mode"], "lexical");

    // With a model that answers, the search is hybrid and embeds only the query's opening.
    let awake = Arc::new(AtomicBool::new(true));
    let given = Arc::new(Mutex::new(Vec::new()));
    let loaded = Arc::new(AtomicBool::new(false));
    let url = model(awake.clone(), loaded, given.clone());
    let args = ["embed", "--root", root.to_str().unwrap(), "--model", "test"]
        .map(str::to_owned)
        .into_iter()
        .chain(["--url".to_owned(), url])
        .collect::<Vec<_>>();
    assert_eq!(command(&args, "").unwrap()["indexed"], 3);
    given.lock().unwrap().clear();
    let long = format!("heron {}", "reeds ".repeat(1_000));
    let found = retrieve(Store::open(root).unwrap(), &quick(&long, false)).unwrap();
    assert_eq!(found["mode"], "hybrid");
    assert!(found["warning"].is_null());
    assert_eq!(*given.lock().unwrap(), [3_000]);

    // A model that has stopped answering costs the search its vectors, not its results
    // or its caller's time.
    awake.store(false, Ordering::SeqCst);
    let started = Instant::now();
    let found = retrieve(Store::open(root).unwrap(), &quick("heron", false)).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(found["mode"], "lexical");
    assert!(found["warning"].is_string());
    assert_eq!(found["results"][0]["id"], "a");
}

#[test]
fn a_prompts_search_asks_only_a_model_that_is_already_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Store::open(root)
        .unwrap()
        .record(&record("a", "the heron stands in the reeds"))
        .unwrap();
    let awake = Arc::new(AtomicBool::new(true));
    let loaded = Arc::new(AtomicBool::new(false));
    let given = Arc::new(Mutex::new(Vec::new()));
    let url = model(awake.clone(), loaded.clone(), given.clone());
    let args = ["embed", "--root", root.to_str().unwrap(), "--model", "test"]
        .map(str::to_owned)
        .into_iter()
        .chain(["--url".to_owned(), url])
        .collect::<Vec<_>>();
    assert_eq!(command(&args, "").unwrap()["indexed"], 1);
    given.lock().unwrap().clear();
    let search = |loaded_only| {
        let request = Retrieval {
            query: "heron",
            exclude: "",
            limit: 8,
            sync: false,
            embed_within: Duration::from_millis(300),
            embed_chars: 3_000,
            embed_loaded_only: loaded_only,
        };
        retrieve(Store::open(root).unwrap(), &request).unwrap()
    };
    // The model is installed but not in memory: a prompt's search does not make it load,
    // and that is no fault to report.
    let found = search(true);
    assert_eq!(found["mode"], "lexical");
    assert!(found["warning"].is_null());
    assert_eq!(found["results"][0]["id"], "a");
    assert!(given.lock().unwrap().is_empty());
    // A search someone asked for loads it.
    assert_eq!(search(false)["mode"], "hybrid");
    assert_eq!(*given.lock().unwrap(), [5]);
    // Once the model is in memory, a prompt's search uses it too.
    loaded.store(true, Ordering::SeqCst);
    assert_eq!(search(true)["mode"], "hybrid");
    assert_eq!(*given.lock().unwrap(), [5, 5]);
    // A server that does not answer counts as not loaded, quickly and quietly.
    awake.store(false, Ordering::SeqCst);
    let started = Instant::now();
    let found = search(true);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(found["mode"], "lexical");
    assert!(found["warning"].is_null());
    assert_eq!(found["results"][0]["id"], "a");
}

#[test]
fn ranking_weighs_rare_words_and_text_length() {
    let long = format!("the {}", "reeds and water ".repeat(60));
    let texts = [
        "the heron stood in the reeds",
        "the reeds",
        long.as_str(),
        "heron heron heron: the heron again, and the heron at dusk",
        "nothing shared",
    ];
    let order = |query: &str| -> Vec<usize> {
        rank(query, &texts)
            .into_iter()
            .map(|(index, _)| index)
            .collect()
    };
    // A word few texts hold outweighs one that most hold, and repeating it helps.
    assert_eq!(order("the heron"), [3, 0, 1, 2]);
    // For a word most texts hold: more mentions first, and a long text's one mention last.
    assert_eq!(order("the"), [0, 3, 1, 2]);
    assert!(order("bittern").is_empty());
    assert!(order("").is_empty());
    assert!(rank("heron", &[]).is_empty());
    // Every score is positive, even for a word every text holds.
    assert!(
        rank("the", &texts[..4])
            .iter()
            .all(|(_, score)| *score > 0.0)
    );
}

#[test]
fn log_hits_and_index_hits_take_turns() {
    let dir = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // Conversations are read from the local log; a registered folder only from the index.
    store.record(&record("a", "heron by the river")).unwrap();
    store
        .record(&record("b", "a heron in the reeds at dusk"))
        .unwrap();
    folder(&store, base.path(), "marsh", "heron");
    let found = retrieve(store, &Retrieval::thorough("heron", "", 8)).unwrap();
    let origins: Vec<&str> = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["origin"].as_str().unwrap())
        .collect();
    assert_eq!(
        origins,
        [
            "conversation:test/one",
            "peer:marsh/memory.nibli",
            "conversation:test/one"
        ]
    );
    assert_eq!(found["results"][0]["id"], "a");
}

#[test]
fn cosine_search_checks_model_dimensions_and_invalidates_changed_sources() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let first = record("first", "feline companion");
    let second = record("second", "railway timetable");
    store.record(&first).unwrap();
    store.record(&second).unwrap();
    store
        .put_vector(&first, "test@digest", &[3.0, 4.0])
        .unwrap();
    store
        .put_vector(&second, "test@digest", &[-4.0, 3.0])
        .unwrap();
    assert!(
        store
            .put_vector(&first, "test@digest", &[f32::NAN, 0.0])
            .is_err()
    );
    assert!(
        store
            .put_vector(&first, "test@digest", &[0.0, 0.0])
            .is_err()
    );
    let found = store
        .search("kitten", "", 1, Some(("test@digest", &[0.6, 0.8])))
        .unwrap();
    assert_eq!(found[0]["id"], "first");
    assert!(
        store
            .search("kitten", "", 1, Some(("test@other", &[0.6, 0.8])))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .search("kitten", "", 1, Some(("test@digest", &[1.0])))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_bad_file_is_skipped_and_named_never_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.record(&record("good", "a findable note")).unwrap();
    std::fs::write(dir.path().join("records/broken.json"), b"{\"origin\": ").unwrap();
    let home = dir.path().join("peer/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    std::fs::write(home.join("notes.md"), b"caf\xff\n").unwrap();
    std::fs::write(home.join("memory.nibli"), "bird(Enma).\n").unwrap();
    store.register("nibli", &home).unwrap();
    let sync = store.sync().unwrap();
    let skipped = sync["skipped"].to_string();
    assert!(skipped.contains("broken.json"), "{sync}");
    assert!(skipped.contains("notes.md"), "{sync}");
    assert_eq!(store.search("findable", "", 8, None).unwrap().len(), 1);
    assert_eq!(store.search("Enma", "", 8, None).unwrap().len(), 1);
}

#[test]
fn restored_peer_conversations_stay_recallable_until_the_peer_files_arrive() {
    let dir = tempfile::tempdir().unwrap();
    let host_a = Store::open(&dir.path().join("host-a")).unwrap();
    let mut private = record("p1", "a private conversation about herons");
    private.origin = "peer:nibli/private-interactions.nibli".into();
    host_a.record(&private).unwrap();
    let manifest = host_a.backup(&dir.path().join("backup")).unwrap();
    let host_b = Store::open(&dir.path().join("host-b")).unwrap();
    host_b.restore(&manifest).unwrap();
    assert_eq!(host_b.search("herons", "", 8, None).unwrap().len(), 1);
    // STORE.md's order on a new host: restore, then register the peers, whose private
    // archive is not in Git.
    let home = dir.path().join("checkout/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    host_b.register("nibli", &home).unwrap();
    host_b.sync().unwrap();
    assert_eq!(host_b.search("herons", "", 8, None).unwrap().len(), 1);
    // Once the peer's own file exists, it wins.
    std::fs::write(home.join("private-interactions.nibli"), "").unwrap();
    host_b.sync().unwrap();
    assert!(host_b.search("herons", "", 8, None).unwrap().is_empty());
}

#[test]
fn restored_copies_stand_in_only_until_a_local_file_has_existed() {
    let dir = tempfile::tempdir().unwrap();
    let host_a = Store::open(&dir.path().join("host-a")).unwrap();
    let mut archive = record("p1", "a private conversation about herons");
    archive.origin = "peer:nibli/private-interactions.nibli".into();
    host_a.record(&archive).unwrap();
    let mut private = record("p2", "a private fact about kingfishers");
    private.origin = "peer:nibli/private.nibli".into();
    host_a.record(&private).unwrap();
    let manifest = host_a.backup(&dir.path().join("backup")).unwrap();
    let host_b = Store::open(&dir.path().join("host-b")).unwrap();
    host_b.restore(&manifest).unwrap();
    let home = dir.path().join("checkout/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    host_b.register("nibli", &home).unwrap();
    host_b.sync().unwrap();
    // Every gitignored private file of the peer, not only the archives.
    assert_eq!(host_b.search("herons", "", 8, None).unwrap().len(), 1);
    assert_eq!(host_b.search("kingfishers", "", 8, None).unwrap().len(), 1);
    std::fs::write(home.join("private-interactions.nibli"), "").unwrap();
    host_b.sync().unwrap();
    assert!(host_b.search("herons", "", 8, None).unwrap().is_empty());
    // A local file has existed, so deleting it empties the origin; the snapshot does not return.
    std::fs::remove_file(home.join("private-interactions.nibli")).unwrap();
    host_b.sync().unwrap();
    assert!(host_b.search("herons", "", 8, None).unwrap().is_empty());
    assert_eq!(host_b.search("kingfishers", "", 8, None).unwrap().len(), 1);
}

#[test]
fn invalid_and_symlinked_records_are_skipped_too() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.record(&record("good", "a findable note")).unwrap();
    let records = dir.path().join("records");
    let mut empty = serde_json::to_value(record("empty", "x")).unwrap();
    empty["payload"]["text"] = json!("");
    std::fs::write(records.join("invalid.json"), empty.to_string()).unwrap();
    let mut tool = serde_json::to_value(record("tool", "tool output")).unwrap();
    tool["payload"]["role"] = json!("tool");
    std::fs::write(records.join("tool.json"), tool.to_string()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hostname", records.join("linked.json")).unwrap();
    let sync = store.sync().unwrap();
    let skipped = sync["skipped"].to_string();
    for name in ["invalid.json", "tool.json"] {
        assert!(skipped.contains(name), "{name}: {sync}");
    }
    #[cfg(unix)]
    assert!(skipped.contains("linked.json"));
    assert_eq!(store.search("findable", "", 8, None).unwrap().len(), 1);
}

#[test]
fn restored_copies_whose_files_are_gone_leave_recall_and_conflicts_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let host_a = Store::open(&dir.path().join("host-a")).unwrap();
    let mut private = record("p2", "a private fact about kingfishers");
    private.origin = "peer:nibli/private.nibli".into();
    host_a.record(&private).unwrap();
    let manifest = host_a.backup(&dir.path().join("backup")).unwrap();
    let root = dir.path().join("host-b");
    let host_b = Store::open(&root).unwrap();
    host_b.restore(&manifest).unwrap();
    let home = dir.path().join("checkout/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    host_b.register("nibli", &home).unwrap();
    host_b.sync().unwrap();
    assert_eq!(host_b.search("kingfishers", "", 8, None).unwrap().len(), 1);
    // The restored record files are removed: their copies leave recall too.
    for entry in std::fs::read_dir(root.join("records")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    host_b.sync().unwrap();
    assert!(
        host_b
            .search("kingfishers", "", 8, None)
            .unwrap()
            .is_empty()
    );
    // A record file that conflicts with the index is skipped and named, not fatal.
    host_b.record(&record("same", "the original text")).unwrap();
    let mut changed =
        serde_json::to_value(record("same", "edited behind the store's back")).unwrap();
    changed["payload"]["text"] = json!("edited behind the store's back");
    std::fs::write(root.join("records/edited-copy.json"), changed.to_string()).unwrap();
    let sync = host_b.sync().unwrap();
    assert!(
        sync["skipped"].to_string().contains("edited-copy.json"),
        "{sync}"
    );
}

#[test]
fn restored_copies_whose_files_vanish_before_the_first_sync_leave_recall() {
    let dir = tempfile::tempdir().unwrap();
    let host_a = Store::open(&dir.path().join("host-a")).unwrap();
    let mut private = record("p3", "a private fact about dippers");
    private.origin = "peer:nibli/private.nibli".into();
    host_a.record(&private).unwrap();
    let manifest = host_a.backup(&dir.path().join("backup")).unwrap();
    let root = dir.path().join("host-b");
    let host_b = Store::open(&root).unwrap();
    host_b.restore(&manifest).unwrap();
    for entry in std::fs::read_dir(root.join("records")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let home = dir.path().join("checkout/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    host_b.register("nibli", &home).unwrap();
    host_b.sync().unwrap();
    host_b.sync().unwrap();
    assert!(host_b.search("dippers", "", 8, None).unwrap().is_empty());
}

#[test]
fn a_record_reaching_an_emptied_origin_still_leaves_with_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    let store = Store::open(&root).unwrap();
    let home = dir.path().join("checkout/lucy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("constitution.nibli"), "# Lucy\n").unwrap();
    store.register("nibli", &home).unwrap();
    // An origin the peer has no file for is emptied and marked once.
    let mut first = record("p4", "a note about wagtails");
    first.origin = "peer:nibli/notes.md".into();
    store.record(&first).unwrap();
    for entry in std::fs::read_dir(root.join("records")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    store.sync().unwrap();
    assert!(store.search("wagtails", "", 8, None).unwrap().is_empty());
    // A later record under the same origin, whose file then goes too, leaves recall as well.
    let mut second = record("p5", "a note about ouzels");
    second.origin = "peer:nibli/notes.md".into();
    store.record(&second).unwrap();
    for entry in std::fs::read_dir(root.join("records")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    store.sync().unwrap();
    store.sync().unwrap();
    assert!(store.search("ouzels", "", 8, None).unwrap().is_empty());
}

#[test]
fn nibli_log_rebuilds_originals_without_json_copies_or_redb() {
    let dir = tempfile::tempdir().unwrap();
    let record = crate::store::Record {
        origin: "conversation:rebuild".into(),
        payload: serde_json::json!({"id":"exact","text":"source\nbytes 🐈","private":true}),
    };
    {
        let store = crate::store::Store::open(dir.path()).unwrap();
        store.record(&record).unwrap();
    }
    std::fs::remove_file(dir.path().join("memory.redb")).unwrap();
    std::fs::remove_dir_all(dir.path().join("records")).unwrap();
    let store = crate::store::Store::open(dir.path()).unwrap();
    store.sync().unwrap();
    assert_eq!(store.get(&record.origin, "exact").unwrap(), Some(record));
}

#[test]
fn a_repeated_backup_adds_nothing_and_a_new_record_adds_only_what_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("host-a")).unwrap();
    // Operations large enough for the local log to span several groups.
    for i in 0..6 {
        let text = format!("message {i} {}", "padding ".repeat(5000));
        store.record(&record(&format!("m{i}"), &text)).unwrap();
    }
    let backup = dir.path().join("backup");
    let first = store.capture(None).unwrap().write(&backup).unwrap();
    assert!(first.changed);
    assert_eq!((first.objects, first.new_objects), (6, 6));
    assert!(first.parts > 2, "{} parts", first.parts);
    assert_eq!(first.new_parts, first.parts);
    assert_eq!(
        manifest_of(&first.manifest)["format"],
        "lucy-memory-snapshot-v2"
    );
    let before = files(&backup);
    let again = store.capture(None).unwrap().write(&backup).unwrap();
    assert!(!again.changed);
    assert_eq!(again.manifest, first.manifest);
    assert_eq!((again.new_objects, again.new_parts), (0, 0));
    assert_eq!(files(&backup), before);
    store
        .record(&record("one-more", "a single new message"))
        .unwrap();
    let third = store.capture(None).unwrap().write(&backup).unwrap();
    assert!(third.changed);
    assert_eq!((third.new_objects, third.new_parts), (1, 1));
    // Every earlier group is carried over as it was; only the last one is new.
    let (old, new) = (parts_of(&first.manifest, 0), parts_of(&third.manifest, 0));
    assert_eq!(old[..old.len() - 1], new[..old.len() - 1]);
    assert!(before.iter().all(|file| files(&backup).contains(file)));
    let restored = Store::open(&dir.path().join("host-b")).unwrap();
    restored.restore(&third.manifest).unwrap();
    assert_eq!(restored.all().unwrap(), store.all().unwrap());
    restored.restore(&third.manifest).unwrap();
    assert_eq!(restored.all().unwrap().len(), 7);
}

#[test]
fn both_older_snapshot_shapes_still_restore() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("host-a");
    let store = Store::open(&root).unwrap();
    store
        .record(&record("first", "original whitespace\n  kept"))
        .unwrap();
    store
        .record(&record("second", "another visible message"))
        .unwrap();
    let backup = dir.path().join("backup");
    std::fs::create_dir_all(backup.join("objects")).unwrap();
    std::fs::create_dir_all(backup.join("snapshots/host-a")).unwrap();
    let mut objects = Vec::new();
    for record in store.all().unwrap() {
        let bytes = serde_json::to_vec(&record).unwrap();
        let hash = crate::store::digest(&bytes);
        std::fs::write(backup.join("objects").join(format!("{hash}.json")), bytes).unwrap();
        objects.push(hash);
    }
    // Records alone, as on 2026-10-20; then with every log's whole text inside the manifest.
    let embedded = crate::replication::backup(&root).unwrap();
    for (name, replication) in [("1.json", Value::Null), ("2.json", embedded)] {
        let mut snapshot = json!({"format":"lucy-memory-snapshot-v1","host":"host-a",
            "created_unix_ns":"1","objects":objects});
        if !replication.is_null() {
            snapshot["replication"] = replication;
        }
        let manifest = backup.join("snapshots/host-a").join(name);
        std::fs::write(&manifest, snapshot.to_string()).unwrap();
        let fresh = Store::open(&dir.path().join(format!("fresh-{name}"))).unwrap();
        assert_eq!(fresh.restore(&manifest).unwrap(), 2);
        assert_eq!(fresh.all().unwrap(), store.all().unwrap());
    }
}

#[test]
fn a_corrupt_missing_or_escaping_part_fails_before_anything_is_restored() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("host-a")).unwrap();
    store.record(&record("first", "a visible message")).unwrap();
    let backup = dir.path().join("backup");
    let written = store.capture(None).unwrap().write(&backup).unwrap();
    let part = backup.join(&parts_of(&written.manifest, 0)[0]);
    let original = std::fs::read(&part).unwrap();
    let refused = |name: &str, manifest: &Path| {
        let root = dir.path().join(name);
        let fresh = Store::open(&root).unwrap();
        assert!(fresh.restore(manifest).is_err(), "{name}");
        assert!(fresh.all().unwrap().is_empty(), "{name}");
        assert!(
            !root.join("replication-v1/local-private.nibli").exists(),
            "{name}"
        );
    };
    std::fs::write(&part, b"corrupted").unwrap();
    refused("corrupt", &written.manifest);
    std::fs::remove_file(&part).unwrap();
    refused("missing", &written.manifest);
    std::fs::write(&part, &original).unwrap();
    // A descriptor pointing outside parts/ is refused whatever that file holds.
    let mut escaping = manifest_of(&written.manifest);
    escaping["replication"]["logs"][0]["archive"]["parts"][0]["path"] = json!("../objects/x.json");
    let bad = written.manifest.with_file_name("9.json");
    std::fs::write(&bad, escaping.to_string()).unwrap();
    refused("escaping", &bad);
    let intact = Store::open(&dir.path().join("intact")).unwrap();
    assert_eq!(intact.restore(&written.manifest).unwrap(), 1);
}

#[test]
fn an_operation_larger_than_one_part_splits_and_restores() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("host-a")).unwrap();
    let large = "é".repeat(2_300_000);
    store.record(&record("large", &large)).unwrap();
    let backup = dir.path().join("backup");
    let written = store.capture(None).unwrap().write(&backup).unwrap();
    let parts = manifest_of(&written.manifest)["replication"]["logs"][0]["archive"]["parts"]
        .as_array()
        .unwrap()
        .clone();
    assert!(parts.len() > 1);
    assert!(
        parts
            .iter()
            .all(|part| part["bytes"].as_u64().unwrap() <= 4 * 1024 * 1024)
    );
    let restored = Store::open(&dir.path().join("host-b")).unwrap();
    restored.restore(&written.manifest).unwrap();
    assert_eq!(restored.all().unwrap(), store.all().unwrap());
}

#[test]
fn no_signing_key_path_or_credential_reaches_a_backup() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("host-a");
    let store = Store::open(&root).unwrap();
    store.record(&record("first", "a visible message")).unwrap();
    enrolled(&root, "kept", "example/kept-memory", true, "KeptLogHeron");
    let key = std::fs::read(root.join("replication-v1/actor.key")).unwrap();
    assert_eq!(key.len(), 32);
    let backup = dir.path().join("backup");
    store.capture(None).unwrap().write(&backup).unwrap();
    let files = files(&backup);
    assert!(!holds(&files, &STANDARD.encode(&key)));
    for (name, bytes) in &files {
        assert!(!bytes.windows(32).any(|w| w == key.as_slice()), "{name}");
    }
    for marker in ["kept-token", "kept-checkout", "nowhere", "actor.key"] {
        assert!(!holds(&files, marker), "{marker}");
    }
    assert!(holds(&files, "KeptLogHeron"));
}

#[test]
fn only_listed_peers_reach_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("host-a");
    let store = Store::open(&root).unwrap();
    // A kept folder, one left out, and the left-out one indexed again under an anonymous name.
    folder(&store, dir.path(), "kept", "KeptHeron");
    folder(&store, dir.path(), "left", "LeftKingfisher");
    folder(&store, dir.path(), "local-0123456789abcdef", "LeftDipper");
    store.sync().unwrap();
    store
        .record(&record("talk", "a conversation that always travels"))
        .unwrap();
    enrolled(&root, "kept", "example/kept-memory", true, "KeptLogHeron");
    enrolled(
        &root,
        "left",
        "elsewhere/left-memory",
        false,
        "LeftLogKingfisher",
    );
    let only = BTreeSet::from(["kept".to_string()]);
    let capture = store.capture(Some(&only)).unwrap();
    assert!(capture.omitted >= 2, "{} omitted", capture.omitted);
    let backup = dir.path().join("backup");
    let written = capture.write(&backup).unwrap();
    let files = files(&backup);
    for marker in [
        "LeftKingfisher",
        "LeftDipper",
        "LeftLogKingfisher",
        "left-scope",
        "elsewhere",
        "left-memory",
        "left-token",
        "peer:left/",
        "local-0123456789abcdef",
    ] {
        assert!(!holds(&files, marker), "{marker} reached the backup");
    }
    for marker in [
        "KeptHeron",
        "KeptLogHeron",
        "kept-scope",
        "a conversation that always travels",
    ] {
        assert!(holds(&files, marker), "{marker} is missing");
    }
    let manifest = manifest_of(&written.manifest);
    assert_eq!(manifest["filter"]["only_peers"], json!(["kept"]));
    let logs: Vec<_> = manifest["replication"]["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|log| log["peer"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(logs, ["kept", "local"]);
    // The unlisted backup of the same store does carry the rest: the list is what kept it out.
    let everything = dir.path().join("everything");
    store.capture(None).unwrap().write(&everything).unwrap();
    assert!(holds(
        &crate::store_tests::files(&everything),
        "LeftLogKingfisher"
    ));
}

#[test]
fn a_backup_refuses_unknown_names_and_smuggled_records_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("host-a");
    let backup = dir.path().join("backup");
    {
        let store = Store::open(&root).unwrap();
        store.record(&record("talk", "a visible message")).unwrap();
        let unknown = BTreeSet::from(["nosuch".to_string()]);
        assert!(store.capture(Some(&unknown)).is_err());
        // A listed log that carries a record belonging to a peer outside the list.
        enrolled(&root, "kept", "example/kept-memory", false, "KeptLogHeron");
        let replica = Replica::open(&root).unwrap();
        let peer = replica.config().unwrap().peers["kept"].clone();
        let smuggled = json!({"record":{"origin":"peer:left/private.nibli",
            "payload":{"id":"s1","private":true,"text":"a record about LeftKingfisher"}}});
        replica
            .append(&peer, "s1", Change::Add, smuggled, json!({}))
            .unwrap();
        drop(replica);
        let only = BTreeSet::from(["kept".to_string()]);
        assert!(store.capture(Some(&only)).is_err());
    }
    let run = |peer: &str| {
        let args = [
            "backup",
            "--root",
            root.to_str().unwrap(),
            "--to",
            backup.to_str().unwrap(),
            "--only-peer",
            peer,
        ]
        .map(str::to_owned);
        command(&args, "")
    };
    assert!(run("nosuch").unwrap_err().contains("unknown peer"));
    assert!(run("kept").unwrap_err().contains("outside the list"));
    assert!(files(&backup).is_empty());
}

#[test]
fn restore_leaves_a_peer_the_snapshot_lacks_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let host_a = dir.path().join("host-a");
    let manifest = {
        let store = Store::open(&host_a).unwrap();
        store.record(&record("talk", "a visible message")).unwrap();
        enrolled(&host_a, "kept", "example/kept-memory", true, "KeptLogHeron");
        enrolled(
            &host_a,
            "left",
            "elsewhere/left-memory",
            false,
            "LeftLogKingfisher",
        );
        let only = BTreeSet::from(["kept".to_string()]);
        let capture = store.capture(Some(&only)).unwrap();
        capture.write(&dir.path().join("backup")).unwrap().manifest
    };
    // Another host holds its own copy of the peer the snapshot left out.
    let host_b = dir.path().join("host-b");
    let store = Store::open(&host_b).unwrap();
    enrolled(
        &host_b,
        "left",
        "elsewhere/left-memory",
        false,
        "HostBKingfisher",
    );
    let mut indexed = record("p1", "an indexed record about HostBDipper");
    indexed.origin = "peer:left/private.nibli".into();
    store.record(&indexed).unwrap();
    let log = host_b.join("replication-v1/left-scope.nibli");
    let state = |store: &Store| {
        let enrollment = Replica::open(&host_b).unwrap().config().unwrap().peers["left"].clone();
        let records: Vec<_> = store
            .all()
            .unwrap()
            .into_iter()
            .filter(|record| record.origin.starts_with("peer:left/"))
            .collect();
        (std::fs::read(&log).unwrap(), enrollment, records)
    };
    let before = state(&store);
    assert_eq!(store.restore(&manifest).unwrap(), 1);
    assert_eq!(state(&store), before);
    let peers = Replica::open(&host_b).unwrap().config().unwrap().peers;
    assert!(peers.contains_key("kept"));
    assert!(!peers["kept"].upload);
}

#[test]
fn an_unlisted_backup_is_refused_after_a_listed_one_unless_all_peers_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("host-a");
    let backup = dir.path().join("backup");
    {
        let store = Store::open(&root).unwrap();
        store.record(&record("talk", "a visible message")).unwrap();
        folder(&store, dir.path(), "kept", "KeptHeron");
    }
    let run = |extra: &[&str]| {
        let mut args = vec![
            "backup",
            "--root",
            root.to_str().unwrap(),
            "--to",
            backup.to_str().unwrap(),
        ];
        args.extend(extra);
        command(&args.into_iter().map(str::to_owned).collect::<Vec<_>>(), "")
    };
    let listed = run(&["--only-peer", "kept"]).unwrap();
    assert_eq!(listed["only_peers"], json!(["kept"]));
    assert_eq!(listed["changed"], true);
    assert!(run(&[]).unwrap_err().contains("--only-peer"));
    assert!(run(&["--only-peer", "kept", "--all-peers"]).is_err());
    let all = run(&["--all-peers"]).unwrap();
    assert_eq!(all["changed"], true);
    assert!(all["only_peers"].is_null());
    // The newest manifest lists nothing again, so a plain run is accepted as before.
    assert_eq!(run(&[]).unwrap()["changed"], false);
    // The list belongs to backup alone.
    let status = [
        "status",
        "--root",
        root.to_str().unwrap(),
        "--only-peer",
        "kept",
    ];
    assert!(command(&status.map(str::to_owned), "").is_err());
}
