use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Mock {
    api: Github,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    requests: Arc<Mutex<Vec<Value>>>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take()
            && let Err(panic) = t.join()
            && !std::thread::panicking()
        {
            std::panic::resume_unwind(panic);
        }
    }
}
impl Mock {
    fn new(
        mut handler: impl FnMut(&str, &str, Value) -> (u16, String, Vec<(String, String)>)
        + Send
        + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let saved = requests.clone();
        let thread = std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                // BSD and Windows may inherit nonblocking mode from the listener.
                // Only accept is polled; complete HTTP requests use blocking reads.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let parts: Vec<_> = first.split_whitespace().collect();
                let method = parts[0];
                let path = parts[1];
                let mut length = 0;
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((k, v)) = line.trim().split_once(':') {
                        if k.eq_ignore_ascii_case("content-length") {
                            length = v.trim().parse().unwrap();
                        }
                        headers.push((k.to_owned(), v.trim().to_owned()));
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                saved
                    .lock()
                    .unwrap()
                    .push(json!({"method":method,"path":path,"body":value,"headers":headers}));
                let (status, body, headers) = handler(method, path, value);
                let extra: String = headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect();
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).unwrap();
            }
        });
        Self {
            api: Github {
                client: Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
                token: Some("test-credential".into()),
                base,
            },
            stop,
            thread: Some(thread),
            requests,
        }
    }
}
fn fixture() -> (tempfile::TempDir, Peer, BTreeMap<String, Operation>) {
    let temp = tempfile::tempdir().unwrap();
    let r = Replica::open(temp.path()).unwrap();
    let mut p = r.local_peer();
    p.name = "test".into();
    p.repository = "example/memory".into();
    p.scope = "test".into();
    r.enroll(p.clone()).unwrap();
    r.append(
        &p,
        "first",
        Change::Add,
        json!({"assertion":"person(Adam)."}),
        json!({}),
    )
    .unwrap();
    let log = r.read(&p).unwrap();
    drop(r);
    (temp, p, log)
}
#[test]
fn conditional_refresh_preserves_valid_cache_and_never_uploads_read_only() {
    let (temp, mut p, log) = fixture();
    p.upload = false;
    let text = encode(&log);
    let manifest=json!({"version":1,"scope":p.scope,"public":p.public,"repository":p.repository,"hashes":{"operations.nibli":digest(text.as_bytes())}}).to_string();
    let mut reads = 0;
    let mock = Mock::new(move |method, path, _| {
        assert_eq!(method, "GET");
        if path.contains("/git/ref/") {
            reads += 1;
            if reads == 1 {
                (
                    200,
                    json!({"ref":"refs/heads/lucy-memory","object":{"sha":"c0"}}).to_string(),
                    vec![("ETag".into(), "etag0".into())],
                )
            } else {
                (304, String::new(), vec![])
            }
        } else if path.contains("manifest.json") {
            (200, manifest.clone(), vec![])
        } else {
            (200, text.clone(), vec![])
        }
    });
    assert_eq!(
        sync_with(temp.path(), &p, &mock.api).unwrap()["status"],
        "fresh"
    );
    assert_eq!(
        sync_with(temp.path(), &p, &mock.api).unwrap()["status"],
        "fresh"
    );
    let requests = mock.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(
        requests[3]["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "if-none-match" && h[1] == "etag0")
    );
}
#[test]
fn corrupt_remote_and_rate_limit_keep_original_log() {
    let (temp, p, original) = fixture();
    let mock = Mock::new(|_, _, _| (429, "{}".into(), vec![("Retry-After".into(), "60".into())]));
    assert!(
        sync_with(temp.path(), &p, &mock.api)
            .unwrap_err()
            .contains("retry after 60")
    );
    assert_eq!(
        Replica::open(temp.path()).unwrap().read(&p).unwrap(),
        original
    );
    let q = p.clone();
    let mock = Mock::new(move |_, path, _| {
        if path.contains("/git/ref/") {
            (
                200,
                json!({"ref":"refs/heads/lucy-memory","object":{"sha":"bad"}}).to_string(),
                vec![],
            )
        } else if path.contains("manifest.json") {
            (200,json!({"version":1,"scope":q.scope,"public":q.public,"repository":q.repository,"hashes":{"operations.nibli":"wrong"}}).to_string(),vec![])
        } else {
            (200, "corrupt".into(), vec![])
        }
    });
    assert!(
        sync_with(temp.path(), &p, &mock.api)
            .unwrap_err()
            .contains("hash mismatch")
    );
    assert_eq!(
        Replica::open(temp.path()).unwrap().read(&p).unwrap(),
        original
    );
}
#[test]
fn concurrent_non_force_push_refetches_and_preserves_both_writers() {
    for version in [1, 2] {
        concurrent_writers(version);
    }
}
fn concurrent_writers(version: u64) {
    let (temp, mut p, _) = fixture();
    let other = tempfile::tempdir().unwrap();
    let r = Replica::open(other.path()).unwrap();
    p.keys.insert(r.actor(), r.public_key());
    r.append(
        &p,
        "other",
        Change::Add,
        json!({"assertion":"person(Bob)."}),
        json!({}),
    )
    .unwrap();
    let concurrent = r.read(&p).unwrap();
    drop(r);
    let q = p.clone();
    let mut raced = false;
    let mut blobs = 0;
    let mut pending = BTreeMap::new();
    let mut published: Option<BTreeMap<String, Operation>> = None;
    let mock = Mock::new(move |method, path, body| {
        if method == "GET" && path == "/repos/example/memory" {
            return (200, json!({"private":true}).to_string(), vec![]);
        }
        if path.contains("/git/ref/") {
            return (
                200,
                json!({"ref":"refs/heads/lucy-memory","object":{"sha":if published.is_some(){"candidate"}else if raced{"c1"}else{"c0"}}})
                    .to_string(),
                vec![],
            );
        }
        let log = if let Some(log) = &published {
            log.clone()
        } else if raced {
            concurrent.clone()
        } else {
            BTreeMap::new()
        };
        let bundle = bundle_for(&q, &log, version);
        if method == "GET" && path.contains("/contents/") {
            let name = path
                .split_once("/contents/")
                .unwrap()
                .1
                .split('?')
                .next()
                .unwrap();
            return (
                200,
                bundle.files.into_iter().find(|(p, _)| p == name).unwrap().1,
                vec![],
            );
        }
        if path.ends_with("/git/blobs") {
            blobs += 1;
            let text =
                String::from_utf8(B64.decode(body["content"].as_str().unwrap()).unwrap()).unwrap();
            if text.contains(", Operation, Json).") {
                pending = decode(&text).unwrap();
            }
            return (
                201,
                json!({"sha":format!("blob{blobs}")}).to_string(),
                vec![],
            );
        }
        if path.ends_with("/git/trees") {
            return (201, json!({"sha":"tree"}).to_string(), vec![]);
        }
        if path.ends_with("/git/commits") {
            return (201, json!({"sha":"candidate"}).to_string(), vec![]);
        }
        assert_eq!(method, "PATCH");
        assert_eq!(body["force"], false);
        if !raced {
            raced = true;
            (409, "{}".into(), vec![])
        } else {
            published = Some(pending.clone());
            (200, "{}".into(), vec![])
        }
    });
    assert_eq!(
        sync_with(temp.path(), &p, &mock.api).unwrap()["uploaded"],
        true
    );
    let log = Replica::open(temp.path()).unwrap().read(&p).unwrap();
    assert_eq!(log.len(), 2);
    let requests = mock.requests.lock().unwrap();
    assert_eq!(
        requests.iter().filter(|r| r["method"] == "PATCH").count(),
        2
    );
    assert!(requests.iter().filter(|r| r["method"] == "PATCH").all(|r| {
        r["path"]
            .as_str()
            .unwrap()
            .ends_with("refs/heads/lucy-memory")
    }));
    assert!(
        requests
            .iter()
            .any(|r| r["path"].as_str().unwrap().ends_with("git/blobs")
                && r["body"]["content"] == B64.encode(encode(&log)))
    );
}

#[test]
fn public_discovery_cannot_name_local_credentials_or_origin_routes() {
    let registry: Value = serde_json::from_str(include_str!("../../../peers.json")).unwrap();
    let peer: Peer = serde_json::from_value(registry["peers"][0].clone()).unwrap();
    public_enrollment(&peer).unwrap();
    let mut planted = peer.clone();
    planted.credential_file = Some("/private/local-token".into());
    assert!(public_enrollment(&planted).is_err());
    let mut planted = peer.clone();
    planted.record_origins.insert("conversation:local".into());
    assert!(public_enrollment(&planted).is_err());
    let mut planted = peer;
    planted.credential_env = Some("LOCAL_SECRET".into());
    assert!(public_enrollment(&planted).is_err());
}

fn memory_server(
    initial: Option<transport::Bundle>,
    corrupt: Arc<AtomicBool>,
    fail_after_publish: bool,
) -> Mock {
    let mut blobs = BTreeMap::<String, String>::new();
    let mut files: BTreeMap<String, String> = initial
        .map(|b| b.files.into_iter().collect())
        .unwrap_or_default();
    let mut tree = BTreeMap::new();
    let mut generation = usize::from(!files.is_empty());
    Mock::new(move |method, path, body| {
        if path == "/repos/example/memory" {
            return (200, json!({"private":true}).to_string(), vec![]);
        }
        if path.contains("/git/ref/") {
            return if generation == 0 {
                (404, "{}".into(), vec![])
            } else {
                (200,json!({"ref":"refs/heads/lucy-memory","object":{"sha":format!("c{generation}")}}).to_string(),vec![])
            };
        }
        if method == "GET" && path.contains("/contents/") {
            let (name, revision) = path
                .split_once("/contents/")
                .unwrap()
                .1
                .split_once('?')
                .unwrap();
            assert_eq!(revision, format!("ref=c{generation}"));
            if corrupt.load(Ordering::SeqCst) && name.starts_with("parts/") {
                return (200, "corrupted part".into(), vec![]);
            }
            return files
                .get(name)
                .map(|s| (200, s.clone(), vec![]))
                .unwrap_or((404, "{}".into(), vec![]));
        }
        if path.ends_with("/git/blobs") {
            let text =
                String::from_utf8(B64.decode(body["content"].as_str().unwrap()).unwrap()).unwrap();
            assert!(text.len() <= transport::PART_BYTES);
            let sha = digest(text.as_bytes());
            blobs.insert(sha.clone(), text);
            return (201, json!({"sha":sha}).to_string(), vec![]);
        }
        if path.ends_with("/git/trees") {
            tree.clear();
            for entry in body["tree"].as_array().unwrap() {
                let path = entry["path"].as_str().unwrap().to_owned();
                assert!(
                    tree.insert(path, blobs[entry["sha"].as_str().unwrap()].clone())
                        .is_none()
                );
            }
            return (201, json!({"sha":"tree"}).to_string(), vec![]);
        }
        if path.ends_with("/git/commits") {
            return (
                201,
                json!({"sha":format!("c{}",generation+1)}).to_string(),
                vec![],
            );
        }
        if method == "PATCH" {
            assert_eq!(body["force"], false);
        } else {
            assert_eq!(body["ref"], "refs/heads/lucy-memory");
        }
        files = tree.clone();
        generation += 1;
        if fail_after_publish && generation == 2 {
            corrupt.store(true, Ordering::SeqCst);
        }
        (200, "{}".into(), vec![])
    })
}
fn bundle_for(p: &Peer, log: &BTreeMap<String, Operation>, version: u64) -> transport::Bundle {
    transport::bundle(json!({"identity":p.name,"repository":p.repository,"scope":p.scope,"public":p.public,"ref":"lucy-memory","schema":"lucy-operations-v1"}),encode(log),snapshot(log).unwrap(),version).unwrap()
}

#[test]
fn large_private_archive_migrates_restores_and_unchanged_sync_does_not_upload() {
    let (temp, p, initial) = fixture();
    let r = Replica::open(temp.path()).unwrap();
    let marker = format!(
        "private-only-sentinel 🐦{}",
        "z".repeat(transport::PART_BYTES)
    );
    r.append(&p,"large",Change::Add,json!({"record":{"origin":"private-test","payload":{"id":"large","text":marker,"private":true}}}),json!({})).unwrap();
    let expected = r.read(&p).unwrap();
    drop(r);
    let mock = memory_server(
        Some(bundle_for(&p, &initial, 1)),
        Arc::new(AtomicBool::new(false)),
        false,
    );
    let report = sync_with(temp.path(), &p, &mock.api).unwrap();
    assert_eq!(report["format"], 2);
    assert_eq!(report["pending"], false);
    let requests = mock.requests.lock().unwrap();
    let first_write = requests.iter().position(|r| r["method"] == "POST").unwrap();
    assert!(
        requests[..first_write]
            .iter()
            .any(|r| r["path"] == "/repos/example/memory")
    );
    let writes = requests.iter().filter(|r| r["method"] != "GET").count();
    drop(requests);
    assert_eq!(
        sync_with(temp.path(), &p, &mock.api).unwrap()["pending"],
        false
    );
    assert_eq!(
        mock.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["method"] != "GET")
            .count(),
        writes
    );
    let restored = tempfile::tempdir().unwrap();
    let mut reader = p.clone();
    reader.upload = false;
    sync_with(restored.path(), &reader, &mock.api).unwrap();
    assert_eq!(
        Replica::open(restored.path())
            .unwrap()
            .read(&reader)
            .unwrap(),
        expected
    );
    let mut public = p.clone();
    public.public = true;
    let rejected =
        Mock::new(|_, _, _| panic!("private content must be rejected before any public request"));
    assert!(rejected.api.publish(&public, None, &expected, 2).is_err());
    assert!(rejected.requests.lock().unwrap().is_empty());
}

#[test]
fn failed_chunk_readback_stays_pending_until_remote_is_verified() {
    let (temp, p, log) = fixture();
    // Starting with v2 forces even small future writes to retain v2.
    let remote = BTreeMap::new();
    let broken = Arc::new(AtomicBool::new(false));
    let mock = memory_server(Some(bundle_for(&p, &remote, 2)), broken.clone(), true);
    // The remote commit lands, but downloading its parts fails. Retain the outbox.
    assert!(sync_with(temp.path(), &p, &mock.api).is_err());
    assert_eq!(Replica::open(temp.path()).unwrap().read(&p).unwrap(), log);
    broken.store(false, Ordering::SeqCst);
    assert_eq!(sync_with(temp.path(), &p, &mock.api).unwrap()["format"], 2);
    // Clear only rebuildable part cache; a damaged remote cannot earn a receipt.
    fs::remove_dir_all(temp.path().join("replication-v1/parts")).unwrap();
    broken.store(true, Ordering::SeqCst);
    assert!(sync_with(temp.path(), &p, &mock.api).is_err());
    broken.store(false, Ordering::SeqCst);
    assert_eq!(
        sync_with(temp.path(), &p, &mock.api).unwrap()["pending"],
        false
    );
}

#[test]
fn github_errors_report_stage_and_size_without_echoing_credentials_or_content() {
    let mock = Mock::new(|_, _, _| {
        (422,json!({"message":"secret private-only-sentinel","errors":[{"value":"test-credential"}]}).to_string(),vec![])
    });
    let error = mock
        .api
        .value(
            Method::POST,
            "/repos/example/memory/git/blobs",
            Some(json!({"content":"private-only-sentinel"})),
        )
        .unwrap_err();
    assert!(
        error.contains("422") && error.contains("git/blobs") && error.contains("request bytes")
    );
    assert!(!error.contains("private-only-sentinel") && !error.contains("test-credential"));
}

#[test]
fn interrupted_download_reuses_only_verified_scope_local_parts() {
    let (temp, mut p, original) = fixture();
    let r = Replica::open(temp.path()).unwrap();
    r.append(&p,"big",Change::Add,json!({"record":{"origin":"private-fixture","payload":{"id":"big","private":true,"text":"z".repeat(transport::PART_BYTES)}}}),json!({})).unwrap();
    let expected = r.read(&p).unwrap();
    drop(r);
    let bundle = bundle_for(&p, &expected, 2);
    let manifest = Manifest::parse(
        &bundle.files.last().unwrap().1,
        &p.repository,
        &p.scope,
        p.public,
    )
    .unwrap();
    let first = manifest.artifact("operations").unwrap().parts[0]
        .path
        .clone();
    let second = manifest.artifact("operations").unwrap().parts[1]
        .path
        .clone();
    let files: BTreeMap<_, _> = bundle.files.into_iter().collect();
    let mut interrupted = false;
    let mock = Mock::new(move |method, path, _| {
        assert_eq!(method, "GET");
        if path.contains("/git/ref/") {
            return (
                200,
                json!({"ref":"refs/heads/lucy-memory","object":{"sha":"fixed"}}).to_string(),
                vec![],
            );
        }
        let name = path
            .split_once("/contents/")
            .unwrap()
            .1
            .split('?')
            .next()
            .unwrap();
        if name == second && !interrupted {
            interrupted = true;
            return (503, "{}".into(), vec![]);
        }
        (200, files[name].clone(), vec![])
    });
    let reader = tempfile::tempdir().unwrap();
    p.upload = false;
    Replica::open(reader.path())
        .unwrap()
        .merge(&p, original.clone())
        .unwrap();
    assert!(sync_with(reader.path(), &p, &mock.api).is_err());
    assert_eq!(
        Replica::open(reader.path()).unwrap().read(&p).unwrap(),
        original
    );
    assert!(
        !reader
            .path()
            .join("replication-v1")
            .join(format!("{}.remote.json", p.scope))
            .exists()
    );
    assert_eq!(
        sync_with(reader.path(), &p, &mock.api).unwrap()["pending"],
        false
    );
    assert_eq!(
        Replica::open(reader.path()).unwrap().read(&p).unwrap(),
        expected
    );
    assert_eq!(
        mock.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["path"].as_str().unwrap().contains(&first))
            .count(),
        1
    );
}

#[test]
fn expired_rate_limit_reset_is_not_mistaken_for_a_multi_year_delay() {
    let mock = Mock::new(|_, _, _| {
        (
            403,
            "{}".into(),
            vec![
                ("X-RateLimit-Remaining".into(), "0".into()),
                ("X-RateLimit-Reset".into(), "1".into()),
            ],
        )
    });
    let error = mock.api.value(Method::GET, "/test", None).unwrap_err();
    assert!(error.contains("retry after 0 (HTTP 403"));
}

#[test]
fn room_archive_import_publish_restore_and_recall_keep_origin() {
    let (temp, p, _) = fixture();
    let archive = lucy_room_memory::RoomArchiveV1 {
        messages: vec![lucy_room_memory::Message {
            id: "room-origin-uuid".into(),
            source: "lucy-room".into(),
            session: "browser-visit".into(),
            role: "user".into(),
            channel: "user".into(),
            speaker: "visitor".into(),
            text: "Remember the lavender bird 🐦\nExactly this line.".into(),
            timestamp: "2026-10-26T10:00:00Z".into(),
            timestamp_kind: "capture".into(),
            private: true,
            provenance: lucy_room_memory::Provenance {
                room: "https://example.test/lucy/room/".into(),
                device: "browser-device".into(),
                status: "complete".into(),
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let bytes = archive.json().unwrap();
    crate::room::import_local(temp.path(), bytes.as_bytes(), &p.name).unwrap();
    let expected = Replica::open(temp.path()).unwrap().read(&p).unwrap();
    let mut blobs = BTreeMap::<String, String>::new();
    let mut tree = BTreeMap::<String, String>::new();
    let mut published = false;
    let mock = Mock::new(move |method, path, body| {
        if path == "/repos/example/memory" {
            return (200, json!({"private":true}).to_string(), vec![]);
        }
        if path.contains("/git/ref/") {
            return if published {
                (200,json!({"ref":"refs/heads/lucy-memory","object":{"sha":"1234567890123456789012345678901234567890"}}).to_string(),vec![])
            } else {
                (404, "{}".into(), vec![])
            };
        }
        if method == "GET" && path.contains("/contents/") {
            let name = path
                .split("/contents/")
                .nth(1)
                .unwrap()
                .split('?')
                .next()
                .unwrap();
            return (200, blobs[&tree[name]].clone(), vec![]);
        }
        if path.ends_with("/git/blobs") {
            let sha = format!("blob{}", blobs.len());
            blobs.insert(
                sha.clone(),
                String::from_utf8(B64.decode(body["content"].as_str().unwrap()).unwrap()).unwrap(),
            );
            return (201, json!({"sha":sha}).to_string(), vec![]);
        }
        if path.ends_with("/git/trees") {
            for entry in body["tree"].as_array().unwrap() {
                tree.insert(
                    entry["path"].as_str().unwrap().into(),
                    entry["sha"].as_str().unwrap().into(),
                );
            }
            return (201, json!({"sha":"tree"}).to_string(), vec![]);
        }
        if path.ends_with("/git/commits") {
            return (
                201,
                json!({"sha":"1234567890123456789012345678901234567890"}).to_string(),
                vec![],
            );
        }
        assert_eq!(path, "/repos/example/memory/git/refs");
        assert_eq!(body["ref"], "refs/heads/lucy-memory");
        published = true;
        (201, "{}".into(), vec![])
    });
    let report = sync_private_with(temp.path(), &p, &mock.api).unwrap();
    assert_eq!(report["pending"], false);
    assert_eq!(report["head"], "1234567890123456789012345678901234567890");
    let restored = tempfile::tempdir().unwrap();
    let rep = Replica::open(restored.path()).unwrap();
    let mut read_peer = p.clone();
    read_peer.upload = false;
    rep.enroll(read_peer.clone()).unwrap();
    drop(rep);
    sync_with(restored.path(), &read_peer, &mock.api).unwrap();
    let rep = Replica::open(restored.path()).unwrap();
    assert_eq!(rep.read(&read_peer).unwrap(), expected);
    drop(rep);
    let recalled = super::super::recall(restored.path(), "lavender", Some(&p.scope)).unwrap();
    assert!(recalled.iter().any(|r|r.to_string().contains("lucy-room")&&r.to_string().contains("browser-visit")));
    let public = Mock::new(|_, _, _| (200, json!({"private":false}).to_string(), vec![]));
    assert!(
        sync_private_with(temp.path(), &p, &public.api)
            .unwrap_err()
            .contains("private repository")
    );
    assert_eq!(public.requests.lock().unwrap().len(), 1);
    // General sync retries reach publish directly, and must refuse before POSTing blobs too.
    assert!(
        public
            .api
            .publish(&p, None, &expected, 1)
            .unwrap_err()
            .contains("private repository")
    );
    assert!(
        public
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r["method"] == "GET")
    );
    let missing = Mock::new(|_, _, _| (503, "{}".into(), vec![]));
    assert!(sync_private_with(temp.path(), &p, &missing.api).is_err());
    assert_eq!(
        Replica::open(temp.path()).unwrap().read(&p).unwrap(),
        expected
    );
}
