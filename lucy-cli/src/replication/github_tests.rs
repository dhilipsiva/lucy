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
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
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
    let mock = Mock::new(move |method, path, body| {
        if path.contains("/git/ref/") {
            return (
                200,
                json!({"ref":"refs/heads/lucy-memory","object":{"sha":if raced{"c1"}else{"c0"}}})
                    .to_string(),
                vec![],
            );
        }
        let log = if raced {
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
