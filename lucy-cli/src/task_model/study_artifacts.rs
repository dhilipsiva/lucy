//! Private, bounded artifacts for replaying study oracles without model prose.
use super::*;
use crate::store::{Record, Store, digest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FILES: usize = 1024;

pub struct Pending {
    directory: PathBuf,
    inputs: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    directory: PathBuf,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    records: Vec<Record>,
    oracle: Vec<Criterion>,
}

fn files(root: &Path, copy_to: Option<&Path>) -> Result<BTreeMap<String, String>> {
    fn walk(
        root: &Path,
        at: &Path,
        destination: Option<&Path>,
        bytes: &mut u64,
        out: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        for entry in fs::read_dir(at).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let name = relative.to_str().ok_or("artifact path is not UTF-8")?;
            checks::validate(&Check::FileExists { path: name.into() })?;
            if kind.is_symlink() {
                return Err("symlink in study artifacts".into());
            }
            if kind.is_dir() {
                if let Some(destination) = destination {
                    fs::create_dir_all(destination.join(relative)).map_err(|e| e.to_string())?;
                }
                walk(root, &path, destination, bytes, out)?;
            } else if kind.is_file() {
                if out.len() >= MAX_FILES {
                    return Err("too many study artifact files".into());
                }
                let length = entry.metadata().map_err(|e| e.to_string())?.len();
                *bytes = bytes.checked_add(length).ok_or("artifact size overflow")?;
                if *bytes > MAX_BYTES {
                    return Err("study artifacts exceed 64 MiB".into());
                }
                use std::io::Read;
                let mut content = Vec::new();
                fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(length + 1)
                    .read_to_end(&mut content)
                    .map_err(|e| e.to_string())?;
                if content.len() as u64 != length {
                    return Err("artifact changed while copying".into());
                }
                out.insert(name.into(), digest(&content));
                if let Some(destination) = destination {
                    crate::store::save(&destination.join(relative), &content)?;
                }
            } else {
                return Err("non-regular study artifact".into());
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, copy_to, &mut 0, &mut out)?;
    Ok(out)
}

pub fn prepare(workspace: &Path, request: &study::TrialRequest) -> Result<Pending> {
    let base = PathBuf::from(std::env::var_os("LUCY_STUDY_ROOT").ok_or("study root missing")?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let parent = base.join("archives");
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    if parent.canonicalize().map_err(|e| e.to_string())? != parent {
        return Err("symlinked study archive parent".into());
    }
    let directory = request
        .archive
        .clone()
        .unwrap_or(parent.join(identity("trial")?));
    if directory.parent() != Some(parent.as_path()) || directory.exists() {
        return Err("study archive must be new and inside the isolated archives directory".into());
    }
    let outputs: Vec<_> = request
        .output_writes
        .iter()
        .map(|w| w.path.as_str())
        .collect();
    let mut produced = false;
    let mut attributed = request.definition.family != "retrieval";
    for criterion in &request.oracle {
        checks::validate(&criterion.check)?;
        match &criterion.check {
            Check::JsonEquals { path, pointer, .. } => {
                produced |= outputs.contains(&path.as_str()) && pointer != "/source";
                attributed |= outputs.contains(&path.as_str()) && pointer == "/source";
            }
            Check::FileSha256 { path, .. } => produced |= outputs.contains(&path.as_str()),
            Check::FileExists { .. } | Check::Record { .. } => {}
            _ => {
                return Err(
                    "study oracle needs independently replayable file or record predicates".into(),
                );
            }
        }
    }
    if !produced || !attributed {
        return Err(
            "study oracle must check produced output and retrieval source attribution".into(),
        );
    }
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    fs::create_dir(directory.join("inputs")).map_err(|e| e.to_string())?;
    let inputs = files(workspace, Some(&directory.join("inputs")))?;
    Ok(Pending { directory, inputs })
}

pub fn finish(
    workspace: &Path,
    pending: &Pending,
    request: &study::TrialRequest,
    store: &Store,
) -> Result<Value> {
    let destination = pending.directory.join("outputs");
    fs::create_dir(&destination).map_err(|e| e.to_string())?;
    let archive = Archive {
        directory: pending.directory.clone(),
        inputs: pending.inputs.clone(),
        outputs: files(workspace, Some(&destination))?,
        records: store
            .all()?
            .into_iter()
            .filter(|r| r.origin.starts_with("fixture:"))
            .collect(),
        oracle: request.oracle.clone(),
    };
    let value = serde_json::to_value(archive).map_err(|e| e.to_string())?;
    crate::store::save(
        &pending.directory.join("manifest.json"),
        &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
    )?;
    Ok(value)
}

pub fn replay(value: &Value) -> Result<bool> {
    let archive: Archive = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let root = archive
        .directory
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if root != archive.directory
        || files(&root.join("inputs"), None)? != archive.inputs
        || files(&root.join("outputs"), None)? != archive.outputs
    {
        return Err("study artifact manifest mismatch".into());
    }
    let mut pass = true;
    for criterion in &archive.oracle {
        checks::validate(&criterion.check)?;
        pass &= match &criterion.check {
            Check::FileExists { path } => archive.outputs.contains_key(path),
            Check::FileSha256 { path, sha256 } => archive.outputs.get(path) == Some(sha256),
            Check::JsonEquals {
                path,
                pointer,
                expected,
            } => {
                if !archive.outputs.contains_key(path) {
                    false
                } else {
                    let content =
                        fs::read(root.join("outputs").join(path)).map_err(|e| e.to_string())?;
                    serde_json::from_slice::<Value>(&content)
                        .ok()
                        .is_some_and(|v| v.pointer(pointer) == Some(expected))
                }
            }
            Check::Record {
                origin,
                record,
                sha256,
            } => archive
                .records
                .iter()
                .any(|r| &r.origin == origin && r.payload["id"] == *record && hash(r) == *sha256),
            _ => return Err("unsupported study replay predicate".into()),
        };
    }
    Ok(pass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replay_checks_bytes_not_just_receipts() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("inputs")).unwrap();
        fs::create_dir(temp.path().join("outputs")).unwrap();
        fs::write(temp.path().join("outputs/result.json"), b"{\"answer\":42}").unwrap();
        let value = json!({"directory":temp.path(),"inputs":{},
            "outputs":{"result.json":digest(b"{\"answer\":42}")},"records":[],
            "oracle":[{"id":"answer","description":"independent answer","check":
                {"kind":"json_equals","path":"result.json","pointer":"/answer","expected":42}}]});
        assert!(replay(&value).unwrap());
        fs::write(temp.path().join("outputs/result.json"), b"{\"answer\":43}").unwrap();
        assert!(replay(&value).is_err());
    }
}
