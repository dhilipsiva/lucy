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
        let text = encode(&log);
        if path.contains("manifest.json") {
            return (200,json!({"version":1,"scope":q.scope,"public":q.public,"repository":q.repository,"hashes":{"operations.nibli":digest(text.as_bytes())}}).to_string(),vec![]);
        }
        if path.contains("operations.nibli") {
            return (200, text, vec![]);
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
    let last_blob = requests
        .iter()
        .filter(|r| r["path"].as_str().unwrap().ends_with("git/blobs"))
        .rev()
        .nth(2)
        .unwrap();
    let published = decode(
        &String::from_utf8(
            B64.decode(last_blob["body"]["content"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(published, log);
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
    let missing = Mock::new(|_, _, _| (503, "{}".into(), vec![]));
    assert!(sync_private_with(temp.path(), &p, &missing.api).is_err());
    assert_eq!(
        Replica::open(temp.path()).unwrap().read(&p).unwrap(),
        expected
    );
}
