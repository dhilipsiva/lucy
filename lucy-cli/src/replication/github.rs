//! GitHub Git-data transport. No checkout, shell, or Git executable is involved.
use super::*;
use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode};
use std::io::Read;
use std::time::Duration;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

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
        if mutation {
            std::thread::sleep(Duration::from_secs(1));
        }
        let response = req
            .send()
            .map_err(|e| format!("GitHub request failed: {}", e.without_url()))?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS
            || (response.status() == StatusCode::FORBIDDEN
                && (response.headers().contains_key("Retry-After")
                    || response
                        .headers()
                        .get("X-RateLimit-Remaining")
                        .is_some_and(|v| v == "0")))
        {
            let retry = response
                .headers()
                .get("Retry-After")
                .or_else(|| response.headers().get("X-RateLimit-Reset"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("later");
            return Err(format!("GitHub rate limit; retry after {retry}"));
        }
        Ok(response)
    }
    fn value(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let response = self.request(method, path, body, None, false)?;
        if !response.status().is_success() {
            return Err(format!("GitHub status {}", response.status().as_u16()));
        }
        serde_json::from_str(&limited(response)?).map_err(err)
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
            return Err(format!("GitHub ref status {}", response.status()));
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
            return Err(format!(
                "GitHub contents status {} for {path}",
                response.status()
            ));
        }
        limited(response)
    }
    fn publish(
        &self,
        peer: &Peer,
        head: Option<&str>,
        log: &BTreeMap<String, Operation>,
    ) -> Result<bool> {
        if self.token.is_none() {
            return Err("upload pending: no configured GitHub credential".into());
        }
        let operations = encode(log);
        let snap = snapshot(log)?;
        let manifest = json!({"version":1,"identity":peer.name,"repository":peer.repository,"ref":"lucy-memory","scope":peer.scope,"public":peer.public,"schema":"lucy-operations-v1","paths":{"operations":"operations.nibli","snapshot":"snapshot.nibli"},"hashes":{"operations.nibli":digest(operations.as_bytes()),"snapshot.nibli":digest(snap.as_bytes())},"keys":peer.keys});
        let files = [
            ("operations.nibli", operations),
            ("snapshot.nibli", snap),
            (
                "manifest.json",
                serde_json::to_string_pretty(&manifest).map_err(err)?,
            ),
        ];
        let mut tree = Vec::new();
        for (path, content) in files {
            // Blobs support archives beyond the inline tree-content API limit.
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
            status => Err(format!("GitHub ref update status {status}")),
        }
    }
}
fn limited(response: Response) -> Result<String> {
    let mut text = String::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(err)?;
    if text.len() as u64 > MAX_BYTES {
        return Err("peer object exceeds 64 MiB limit".into());
    }
    Ok(text)
}

pub fn sync_peer(root: &Path, peer: &Peer) -> Result<Value> {
    peer.validate()?;
    let api = Github::new(peer)?;
    sync_with(root, peer, &api)
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
        let remote = if let Some(head) = &head {
            if unchanged {
                let path = root
                    .join("replication-v1")
                    .join(format!("{}.remote.nibli", peer.scope));
                decode(&fs::read_to_string(path).map_err(err)?)?
            } else {
                let manifest: Value =
                    serde_json::from_str(&api.contents(peer, "manifest.json", head)?)
                        .map_err(err)?;
                if manifest["version"] != 1
                    || manifest["scope"] != peer.scope
                    || manifest["public"] != peer.public
                    || manifest["repository"] != peer.repository
                {
                    return Err("remote manifest identity or scope mismatch".into());
                }
                let text = api.contents(peer, "operations.nibli", head)?;
                if manifest["hashes"]["operations.nibli"] != digest(text.as_bytes()) {
                    return Err("remote log hash mismatch".into());
                }
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
        save(&state_path,&serde_json::to_vec(&json!({"head":head,"etag":tag.or_else(||state["etag"].as_str().map(str::to_owned)),"checked_at":now()})).map_err(err)?)?;
        drop(replica);
        if !peer.upload || remote == log {
            return Ok(
                json!({"peer":peer.name,"status":"fresh","pending":false,"operations":log.len(),"head":head}),
            );
        }
        if api.publish(peer, head.as_deref(), &log)? {
            let pending = Replica::open(root)?.read(peer)? != log;
            return Ok(
                json!({"peer":peer.name,"status":"fresh","pending":pending,"uploaded":true,"operations":log.len()}),
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
