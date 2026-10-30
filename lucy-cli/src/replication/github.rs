//! GitHub Git-data transport. No checkout, shell, or Git executable is involved.
use super::*;
use lucy_room_memory::transport::{self, LEGACY_BYTES, MANIFEST_BYTES, Manifest};
use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode};
use std::io::Read;
use std::time::Duration;

pub struct Github {
    client: Client,
    token: Option<String>,
    base: String,
}
impl Github {
    pub fn new(peer: &Peer) -> Result<Self> {
        let token = if let Some(path) = &peer.credential_file {
            if fs::symlink_metadata(path)
                .map_err(err)?
                .file_type()
                .is_symlink()
            {
                return Err("credential file cannot be a symlink".into());
            }
            Some(
                fs::read_to_string(path)
                    .map_err(|_| "credential file unavailable")?
                    .trim()
                    .to_string(),
            )
        } else {
            peer.credential_env
                .as_ref()
                .map(|name| {
                    std::env::var(name)
                        .map_err(|_| format!("credential environment {name} unavailable"))
                })
                .transpose()?
        };
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(err)?,
            token,
            base: "https://api.github.com".into(),
        })
    }
    fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        etag: Option<&str>,
        raw: bool,
    ) -> Result<Response> {
        let mutation = method != Method::GET;
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .header("User-Agent", "lucy/0.3.0")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(
                "Accept",
                if raw {
                    "application/vnd.github.raw+json"
                } else {
                    "application/vnd.github+json"
                },
            );
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(etag) = etag {
            req = req.header("If-None-Match", etag);
        }
        if let Some(body) = body {
            req = req.json(&body);
        }
        if mutation && !cfg!(test) {
            std::thread::sleep(Duration::from_secs(1));
        }
        let response = req
            .send()
            .map_err(|e| format!("GitHub request failed at {path}: {}", e.without_url()))?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS
            || (response.status() == StatusCode::FORBIDDEN
                && (response.headers().contains_key("Retry-After")
                    || response
                        .headers()
                        .get("X-RateLimit-Remaining")
                        .is_some_and(|v| v == "0")))
        {
            let number = |name: &str| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
            };
            let seconds = now().parse::<u64>().unwrap_or(0) / 1000;
            let retry = number("Retry-After")
                .or_else(|| number("X-RateLimit-Reset").map(|reset| reset.saturating_sub(seconds)))
                .unwrap_or(60);
            return Err(format!(
                "GitHub rate limit; retry after {retry} (HTTP {} at {path})",
                response.status().as_u16()
            ));
        }
        Ok(response)
    }
    fn value(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let bytes = body.as_ref().map_or(0, |v| v.to_string().len());
        let response = self.request(method, path, body, None, false)?;
        if !response.status().is_success() {
            return Err(response_error(response, path, bytes));
        }
        serde_json::from_str(&bounded(response, MANIFEST_BYTES)?).map_err(err)
    }
    fn ref_head(
        &self,
        peer: &Peer,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<(Option<String>, Option<String>, bool)> {
        let response = self.request(
            Method::GET,
            &format!("/repos/{}/git/ref/heads/{branch}", peer.repository),
            None,
            etag,
            false,
        )?;
        let tag = response
            .headers()
            .get("etag")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok((None, tag, true));
        }
        if response.status() == StatusCode::NOT_FOUND {
            return Ok((None, tag, false));
        }
        if !response.status().is_success() {
            return Err(response_error(response, "read memory ref", 0));
        }
        let v: Value = serde_json::from_str(&limited(response)?).map_err(err)?;
        if v["ref"] != format!("refs/heads/{branch}") {
            return Err("GitHub returned an unexpected ref".into());
        }
        Ok((
            Some(v["object"]["sha"].as_str().ok_or("missing commit")?.into()),
            tag,
            false,
        ))
    }
    pub fn contents(&self, peer: &Peer, path: &str, revision: &str) -> Result<String> {
        self.contents_bounded(peer, path, revision, LEGACY_BYTES)
    }
    fn contents_bounded(
        &self,
        peer: &Peer,
        path: &str,
        revision: &str,
        limit: usize,
    ) -> Result<String> {
        // Query parameters are encoded by reqwest::Url, never interpolated as URL syntax.
        let mut url = reqwest::Url::parse(&format!(
            "{}/repos/{}/contents/",
            self.base, peer.repository
        ))
        .map_err(err)?;
        url.path_segments_mut()
            .map_err(|_| "bad API URL")?
            .pop_if_empty()
            .extend(path.split('/'));
        url.query_pairs_mut().append_pair("ref", revision);
        let path = &url.as_str()[self.base.len()..];
        let response = self.request(Method::GET, path, None, None, true)?;
        if !response.status().is_success() {
            return Err(response_error(response, path, 0));
        }
        bounded(response, limit)
    }
    fn manifest(&self, peer: &Peer, head: &str) -> Result<Manifest> {
        Manifest::parse(
            &self.contents_bounded(peer, "manifest.json", head, MANIFEST_BYTES)?,
            &peer.repository,
            &peer.scope,
            peer.public,
        )
    }
    fn archive(
        &self,
        root: &Path,
        peer: &Peer,
        head: &str,
        manifest: &Manifest,
        name: &str,
        cache: bool,
    ) -> Result<String> {
        let mut assembler = manifest.assembler(name)?;
        for part in &manifest.artifact(name)?.parts {
            let path = root
                .join("replication-v1")
                .join("parts")
                .join(&peer.scope)
                .join(format!("{}.txt", part.sha256));
            let cached = if cache && manifest.version == 2 {
                fs::File::open(&path).ok().and_then(|file| {
                    let mut text = String::new();
                    file.take(part.bytes as u64 + 1)
                        .read_to_string(&mut text)
                        .ok()?;
                    (text.len() == part.bytes && digest(text.as_bytes()) == part.sha256)
                        .then_some(text)
                })
            } else {
                None
            };
            let text = match cached {
                Some(text) => text,
                None => self.contents_bounded(peer, &part.path, head, part.bytes)?,
            };
            assembler.push(&text)?;
            if manifest.version == 2 {
                save(&path, text.as_bytes())?;
            }
        }
        assembler.finish()
    }
    fn publish(
        &self,
        peer: &Peer,
        head: Option<&str>,
        log: &BTreeMap<String, Operation>,
        previous_format: u64,
    ) -> Result<bool> {
        validate(log, peer)?;
        if !peer.upload {
            return Err("upload refused for read-only peer".into());
        }
        if self.token.is_none() {
            return Err("upload pending: no configured GitHub credential".into());
        }
        // A pending private outbox can also be retried by ordinary memory sync.
        // Check actual repository visibility at every upload boundary, before any blob.
        if !peer.public {
            let repository =
                self.value(Method::GET, &format!("/repos/{}", peer.repository), None)?;
            if repository["private"] != true {
                return Err(
                    "Private upload refused: GitHub does not report a private repository".into(),
                );
            }
        }
        let operations = encode(log);
        let snap = snapshot(log)?;
        let metadata = json!({"identity":peer.name,"repository":peer.repository,"ref":"lucy-memory","scope":peer.scope,"public":peer.public,"schema":"lucy-operations-v1","keys":peer.keys});
        let bundle = transport::bundle(metadata, operations, snap, previous_format)?;
        let mut tree = Vec::new();
        for (path, content) in bundle.files {
            let blob = self.value(
                Method::POST,
                &format!("/repos/{}/git/blobs", peer.repository),
                Some(json!({"content":B64.encode(content),"encoding":"base64"})),
            )?;
            tree.push(json!({"path":path,"mode":"100644","type":"blob","sha":blob["sha"]}));
        }
        let tree = self.value(
            Method::POST,
            &format!("/repos/{}/git/trees", peer.repository),
            Some(json!({"tree":tree})),
        )?;
        let parents: Vec<_> = head.into_iter().collect();
        let commit=self.value(Method::POST,&format!("/repos/{}/git/commits",peer.repository),Some(json!({"message":"Lucy scoped memory synchronization","tree":tree["sha"],"parents":parents})))?;
        let (method, path, body) = if head.is_some() {
            (
                Method::PATCH,
                format!("/repos/{}/git/refs/heads/lucy-memory", peer.repository),
                json!({"sha":commit["sha"],"force":false}),
            )
        } else {
            (
                Method::POST,
                format!("/repos/{}/git/refs", peer.repository),
                json!({"ref":"refs/heads/lucy-memory","sha":commit["sha"]}),
            )
        };
        let response = self.request(method, &path, Some(body), None, false)?;
        match response.status() {
            StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY => Ok(false),
            status if status.is_success() => Ok(true),
            _ => Err(response_error(response, "update memory ref", 0)),
        }
    }
}
fn limited(response: Response) -> Result<String> {
    bounded(response, LEGACY_BYTES)
}
fn bounded(response: Response, limit: usize) -> Result<String> {
    let mut text = String::new();
    response
        .take(limit as u64 + 1)
        .read_to_string(&mut text)
        .map_err(err)?;
    if text.len() > limit {
        return Err(format!("peer object exceeds {limit} byte limit"));
    }
    Ok(text)
}

fn response_error(response: Response, stage: &str, bytes: usize) -> String {
    let status = response.status().as_u16();
    let mut body = String::new();
    let _ = response.take(8192).read_to_string(&mut body);
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    // Never echo arbitrary remote strings: validation errors can contain submitted content.
    let message = match value["message"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "your input was too large to process" | "your input was too large to process." => {
            "input too large to process"
        }
        "validation failed" => "validation failed",
        "bad credentials" => "bad credentials",
        "not found" => "not found",
        "resource not accessible by integration" => "resource not accessible by integration",
        _ => "request rejected (response content omitted)",
    };
    format!("GitHub status {status} at {stage}; request bytes {bytes}; {message}")
}

pub fn sync_peer(root: &Path, peer: &Peer) -> Result<Value> {
    peer.validate()?;
    let api = Github::new(peer)?;
    sync_with(root, peer, &api)
}
/// Room archives may be published only after the remote also reports a private repository.
pub fn sync_private_peer(root: &Path, peer: &Peer) -> Result<Value> {
    crate::room::private_target(peer)?;
    let api = Github::new(peer)?;
    sync_private_with(root, peer, &api)
}
fn sync_private_with(root: &Path, peer: &Peer, api: &Github) -> Result<Value> {
    crate::room::private_target(peer)?;
    let repository = api.value(Method::GET, &format!("/repos/{}", peer.repository), None)?;
    if repository["private"] != true {
        return Err("Room upload refused: GitHub does not report a private repository".into());
    }
    sync_with(root, peer, api)
}
fn sync_with(root: &Path, peer: &Peer, api: &Github) -> Result<Value> {
    let replica = Replica::open(root)?;
    let state_path = replica.root.join(format!("{}.remote.json", peer.scope));
    let state: Value = fs::read(&state_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}));
    drop(replica);
    for attempt in 0..3 {
        let etag = if attempt == 0 {
            state["etag"].as_str()
        } else {
            None
        };
        let (head, tag, unchanged) = api.ref_head(peer, "lucy-memory", etag)?;
        let head = if unchanged {
            state["head"].as_str().map(str::to_owned)
        } else {
            head
        };
        if head.is_none() && !peer.upload {
            return legacy(root, api, peer);
        }
        let mut format = state["format"].as_u64().unwrap_or(1);
        let remote = if let Some(head) = &head {
            if unchanged {
                let path = root
                    .join("replication-v1")
                    .join(format!("{}.remote.nibli", peer.scope));
                decode(&fs::read_to_string(path).map_err(err)?)?
            } else {
                let manifest = api.manifest(peer, head)?;
                format = manifest.version;
                let text = api.archive(root, peer, head, &manifest, "operations", true)?;
                let log = decode(&text)?;
                validate(&log, peer)?;
                log
            }
        } else {
            BTreeMap::new()
        };
        let replica = Replica::open(root)?;
        let log = replica.merge(peer, remote.clone())?;
        save(
            &replica.root.join(format!("{}.remote.nibli", peer.scope)),
            encode(&remote).as_bytes(),
        )?;
        save(&state_path,&serde_json::to_vec(&json!({"head":head,"etag":tag.or_else(||state["etag"].as_str().map(str::to_owned)),"checked_at":now(),"format":format})).map_err(err)?)?;
        drop(replica);
        if !peer.upload || remote == log {
            // A prior ref update may have succeeded while its readback failed.
            // Equality of operation logs alone is not a publication receipt.
            if peer.upload
                && let Some(head) = &head
            {
                let manifest = api.manifest(peer, head)?;
                if api.archive(root, peer, head, &manifest, "snapshot", true)? != snapshot(&log)? {
                    return Err(
                        "remote snapshot does not match verified operations; outbox retained"
                            .into(),
                    );
                }
            }
            return Ok(
                json!({"peer":peer.name,"status":"fresh","pending":false,"operations":log.len(),"head":head,"format":format}),
            );
        }
        if api.publish(peer, head.as_deref(), &log, format)? {
            // Read the remote ref and immutable objects back before issuing a commit receipt.
            let (verified_head, tag, _) = api.ref_head(peer, "lucy-memory", None)?;
            let verified_head =
                verified_head.ok_or("published branch not visible; outbox retained")?;
            let manifest = api.manifest(peer, &verified_head)?;
            let text = api.archive(root, peer, &verified_head, &manifest, "operations", false)?;
            let remote_snapshot =
                api.archive(root, peer, &verified_head, &manifest, "snapshot", false)?;
            let verified = decode(&text)?;
            validate(&verified, peer)?;
            if remote_snapshot != snapshot(&verified)? {
                return Err(
                    "published snapshot does not match verified operations; outbox retained".into(),
                );
            }
            if !log
                .iter()
                .all(|(id, operation)| verified.get(id) == Some(operation))
            {
                return Err("published commit does not contain the complete outbox; retry".into());
            }
            let replica = Replica::open(root)?;
            let merged = replica.merge(peer, verified.clone())?;
            save(
                &replica.root.join(format!("{}.remote.nibli", peer.scope)),
                encode(&verified).as_bytes(),
            )?;
            save(
                &state_path,
                &serde_json::to_vec(&json!({"head":verified_head,"etag":tag,"checked_at":now(),"format":manifest.version}))
                    .map_err(err)?,
            )?;
            return Ok(
                json!({"peer":peer.name,"status":"fresh","pending":merged!=verified,"uploaded":true,"operations":verified.len(),"head":verified_head,"format":manifest.version}),
            );
        }
    }
    Err("remote changed during three non-force updates; local outbox retained".into())
}

/// Legacy code branches are read-only evidence until their owner publishes a
/// signed memory branch. Fetch immutable commit content, never a mixed revision.
fn legacy(root: &Path, api: &Github, peer: &Peer) -> Result<Value> {
    let (head, _, _) = api.ref_head(peer, &peer.source_ref, None)?;
    let head = head.ok_or("legacy source branch unavailable")?;
    let mut sources = BTreeMap::new();
    for path in &peer.source_paths {
        let text = api.contents(peer, path, &head)?;
        let replica = Replica::open(root)?;
        let hash = digest(text.as_bytes());
        save(
            &replica.root.join("legacy").join(format!("{hash}.nibli")),
            text.as_bytes(),
        )?;
        // A malformed source never replaces a previously validated manifest.
        if path.ends_with("interactions.nibli") {
            crate::interactions::decode(&text, path.contains("private"))?;
        } else if path.ends_with(".nibli") {
            nibli_engine::NibliEngine::new()
                .assert_text(&text)
                .map_err(err)?;
        }
        sources.insert(path.clone(), hash);
    }
    let replica = Replica::open(root)?;
    save(
        &replica.root.join(format!("{}.legacy.json", peer.scope)),
        &serde_json::to_vec(&json!({"revision":head,"sources":sources})).map_err(err)?,
    )?;
    Ok(
        json!({"peer":peer.name,"status":"fresh","format":"legacy_source","head":head,"pending":false}),
    )
}

fn public_enrollment(peer: &Peer) -> Result<()> {
    peer.validate()?;
    if !peer.public
        || peer.upload
        || peer.local_path.is_some()
        || peer.credential_env.is_some()
        || peer.credential_file.is_some()
        || !peer.record_origins.is_empty()
    {
        return Err("registry contains a nonpublic enrollment".into());
    }
    Ok(())
}

/// Refresh public discovery only. No remote field can enable upload, select a
/// local path, add credentials, or enroll private data.
pub fn discover(root: &Path) -> Result<Value> {
    let embedded: Value = serde_json::from_str(include_str!("../../../peers.json")).map_err(err)?;
    let origin: Peer = serde_json::from_value(embedded["peers"][0].clone()).map_err(err)?;
    let api = Github::new(&origin)?;
    let fetched = api.contents(&origin, "peers.json", "main");
    let (registry, status) = match fetched {
        Ok(text) => (serde_json::from_str::<Value>(&text).map_err(err)?, "fresh"),
        Err(_) => (embedded, "cached"),
    };
    if registry["version"] != 1 {
        return Err("unsupported public registry".into());
    }
    let replica = Replica::open(root)?;
    let config = replica.config()?;
    for value in registry["peers"]
        .as_array()
        .ok_or("registry peers required")?
    {
        let peer: Peer = serde_json::from_value(value.clone()).map_err(err)?;
        public_enrollment(&peer)?;
        // Existing private/write registrations and pinned keys remain local decisions.
        if !config.peers.contains_key(&peer.name) {
            replica.enroll(peer)?;
        }
    }
    Ok(json!({"registry":status}))
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
