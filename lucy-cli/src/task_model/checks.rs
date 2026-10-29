use super::*;
use crate::store::{Store, digest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_JSON: u64 = 32 * 1024 * 1024;
const MAX_FILE: u64 = 256 * 1024 * 1024;
const LEAN_THEOREMS: &[&str] = &[
    "conjunction_total",
    "disjunction_total",
    "negation_total",
    "conjunction_true_iff",
    "conjunction_false_iff",
    "disjunction_true_iff",
    "disjunction_false_iff",
    "conjunction_true_identity",
    "disjunction_false_identity",
    "resource_precedes_unknown",
    "indeterminate_reasons_left_biased",
    "negation_true_iff",
    "negation_false_iff",
    "negation_preserves_indeterminacy",
];
const CRATES: &[&str] = &[
    "nibli-types",
    "nibli-lexicon",
    "nibli-kr",
    "nibli-semantics",
    "nibli-reason",
    "nibli-session",
];

pub(crate) fn validate(check: &Check) -> Result<()> {
    match check {
        Check::FileExists { path } | Check::NibliReport { path } | Check::LeanReport { path } => {
            relative(path)
        }
        Check::FileSha256 { path, sha256 } => {
            relative(path)?;
            if !is_digest(sha256) {
                return Err("file assertion requires a SHA-256 digest".into());
            }
            Ok(())
        }
        Check::JsonEquals {
            path,
            pointer,
            expected,
        } => {
            relative(path)?;
            if pointer.len() > 4096
                || !(pointer.is_empty() || pointer.starts_with('/'))
                || expected.to_string().len() > 4096
            {
                return Err("bounded JSON pointer and expected value required".into());
            }
            Ok(())
        }
        Check::Record {
            origin,
            record,
            sha256,
        } => {
            if !bounded(origin) || !bounded(record) || !is_digest(sha256) {
                return Err("record assertion requires origin, ID and digest".into());
            }
            Ok(())
        }
        Check::Reported { statement } if bounded(statement) => Ok(()),
        _ => Err("bounded manual requirement required".into()),
    }
}

fn relative(path: &str) -> Result<()> {
    if !bounded(path)
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("evidence paths must be relative files without traversal".into());
    }
    Ok(())
}

pub(crate) fn workspace_digest(workspace: &Path) -> Result<String> {
    let root = workspace.canonicalize().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    crate::desktop::actions::path(&root, ".", true)?;
    if !root.is_dir() {
        return Err("workspace must be a directory".into());
    }
    Ok(digest(root.as_os_str().as_encoded_bytes()))
}

pub(crate) fn same_workspace(task: &Task, workspace: &Path) -> Result<()> {
    if task.scope.workspace != workspace_digest(workspace)? {
        return Err("task belongs to a different workspace".into());
    }
    Ok(())
}

fn path(root: &Path, name: &str) -> Result<PathBuf> {
    relative(name)?;
    #[cfg(unix)]
    {
        crate::desktop::actions::path(root, name, false)
    }
    #[cfg(not(unix))]
    {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut result = root.clone();
        for part in Path::new(name).components() {
            let value = part.as_os_str().to_string_lossy();
            if value.starts_with('.') || matches!(value.as_ref(), "credentials.toml") {
                return Err("private path refused".into());
            }
            result.push(part);
            if std::fs::symlink_metadata(&result).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("symlink refused".into());
            }
        }
        Ok(result)
    }
}

struct Inspector<'a> {
    root: &'a Path,
    deadline: Instant,
    evidence: Evidence,
}
impl<'a> Inspector<'a> {
    fn new(root: &'a Path, remaining_ms: u64) -> Self {
        Self {
            root,
            deadline: Instant::now() + Duration::from_millis(remaining_ms),
            evidence: Evidence {
                class: EvidenceClass::Observed,
                files: BTreeMap::new(),
                records: BTreeMap::new(),
                missing_files: vec![],
                missing_records: vec![],
                reason: "inspected".into(),
                replay_available: true,
            },
        }
    }
    fn alive(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            Err("check_budget_exhausted".into())
        } else {
            Ok(())
        }
    }
    fn file(&mut self, name: &str, json: bool) -> Result<(String, Vec<u8>)> {
        self.alive()?;
        let location = path(self.root, name)?;
        if !std::fs::metadata(&location)
            .map_err(|_| "artifact_unavailable")?
            .is_file()
        {
            return Err("artifact_is_not_regular".into());
        }
        let file = std::fs::File::open(location).map_err(|_| "artifact_unavailable")?;
        let metadata = file
            .metadata()
            .map_err(|_| "artifact_metadata_unavailable")?;
        let limit = if json { MAX_JSON } else { MAX_FILE };
        if !metadata.is_file() || metadata.len() > limit {
            return Err("artifact_exceeds_bound_or_is_not_regular".into());
        }
        let mut reader = file.take(limit + 1);
        let mut hasher = Sha256::new();
        let mut bytes = Vec::new();
        let mut chunk = [0; 65536];
        let mut size = 0;
        loop {
            self.alive()?;
            let count = reader
                .read(&mut chunk)
                .map_err(|_| "artifact_read_failed")?;
            if count == 0 {
                break;
            }
            size += count as u64;
            if size > limit {
                return Err("artifact_grew_beyond_bound".into());
            }
            hasher.update(&chunk[..count]);
            if json {
                bytes.extend_from_slice(&chunk[..count]);
            }
        }
        let hash = format!("{:x}", hasher.finalize());
        self.evidence.files.insert(name.into(), hash.clone());
        Ok((hash, bytes))
    }
    fn json(&mut self, name: &str) -> Result<Value> {
        serde_json::from_slice(&self.file(name, true)?.1)
            .map_err(|_| "invalid_json_artifact".into())
    }
    fn hashes(&mut self, value: &Value) -> Result<()> {
        let map = value
            .as_object()
            .filter(|m| !m.is_empty() && m.len() <= 4096)
            .ok_or("missing_or_oversized_hash_manifest")?;
        for (name, expected) in map {
            let expected = expected
                .as_str()
                .filter(|s| is_digest(s))
                .ok_or("invalid_artifact_digest")?;
            if self.file(name, false)?.0 != expected {
                return Err("artifact_digest_mismatch".into());
            }
        }
        Ok(())
    }
    fn extraction(&mut self, report: &Value) -> Result<()> {
        let extraction = &report["extraction"];
        let name = extraction["llbc_file"]
            .as_str()
            .ok_or("missing_extraction")?;
        if self.file(name, false)?.0 != extraction["llbc_sha256"] {
            return Err("extraction_digest_mismatch".into());
        }
        Ok(())
    }
    fn report(&mut self, name: &str, nibli: bool) -> Result<Verdict> {
        self.evidence.class = EvidenceClass::Inspected;
        let report = self.json(name)?;
        if report["schema"] != 1
            || report["component"] != "verdict_algebra"
            || report["self_check_equivalence_verified"] != false
        {
            return Err("unsupported_verification_report_scope".into());
        }
        let sources = report["sources"]
            .as_object()
            .ok_or("missing_source_manifest")?;
        if sources.keys().cloned().collect::<BTreeSet<_>>() != source_paths(self.root, nibli)? {
            return Err("incomplete_or_changed_source_inventory".into());
        }
        self.hashes(&report["sources"])?;
        self.extraction(&report)?;
        if nibli {
            self.hashes(&report["artifacts"])?;
            self.hashes(&report["binaries"])?;
            if report["extraction"]["status"] != "extracted"
                || report["lean_used_as_oracle"] != false
            {
                return Err("incomplete_nibli_extraction".into());
            }
            let result_path = report["artifacts"]
                .as_object()
                .unwrap()
                .keys()
                .find(|p| p.ends_with("/result.json"))
                .ok_or("missing_nibli_result")?;
            let result = self.json(result_path)?;
            let cases = result["cases"].as_array().ok_or("missing_nibli_cases")?;
            if result["schema"] != 1
                || result["verification_kind"] != "exhaustive_finite_domain"
                || result["self_check_equivalence_verified"] != false
                || result["input_values"] != 10
                || result["expected_executions"] != 210
                || cases.len() != 210
                || result["status"] != report["status"]
                || result["status"] != report["nibli"]["status"]
            {
                return Err("incomplete_or_conflicting_nibli_result".into());
            }
            let mut ids = BTreeSet::new();
            for case in cases {
                if !ids.insert(case["case"]["id"].as_str().ok_or("missing_case_id")?) {
                    return Err("duplicate_nibli_case".into());
                }
            }
            let expected_ids: BTreeSet<_> = (0..210)
                .map(|i| {
                    format!(
                        "{}:{i}",
                        if i < 100 {
                            "and"
                        } else if i < 200 {
                            "or"
                        } else {
                            "not"
                        }
                    )
                })
                .collect();
            if ids != expected_ids.iter().map(String::as_str).collect() {
                return Err("incomplete_nibli_case_inventory".into());
            }
            let verdict = match report["status"].as_str() {
                Some("verified")
                    if cases.iter().all(|c| {
                        c["status"] == "verified"
                            && c["actual"] == c["expected"]
                            && c["actual"].as_array().is_some_and(|a| a.len() == 1)
                    }) =>
                {
                    Verdict::Success
                }
                Some("counterexample") if cases.iter().any(|c| c["status"] == "counterexample") => {
                    Verdict::Failure
                }
                _ => return Err("nibli_result_is_inconclusive".into()),
            };
            if report["run"]["exit_code"] != if verdict == Verdict::Success { 0 } else { 1 } {
                return Err("nibli_exit_and_report_disagree".into());
            }
            self.evidence.reason = "inspected_nibli_finite_verdict_report_not_a_fresh_proof".into();
            Ok(verdict)
        } else {
            self.hashes(&report["extraction"]["artifacts"])?;
            if report["extraction"]["status"] != "translated"
                || report["component_verified"] != true
            {
                return Err("lean_component_not_verified".into());
            }
            for step in ["build", "proof", "audit", "lean-version"] {
                if report[step]["status"] != "ok" || report[step]["exit_code"] != 0 {
                    return Err("incomplete_lean_receipt".into());
                }
                let log = report[step]["log"].as_str().ok_or("missing_lean_log")?;
                self.file(log, false)?;
            }
            let axioms = report["axioms"]
                .as_object()
                .filter(|m| m.len() == 14)
                .ok_or("missing_lean_axiom_audit")?;
            if axioms.keys().cloned().collect::<BTreeSet<_>>()
                != LEAN_THEOREMS
                    .iter()
                    .map(|name| format!("NibliSelfCheck.{name}"))
                    .collect()
            {
                return Err("incomplete_lean_theorem_inventory".into());
            }
            if axioms.values().any(|v| {
                v.as_array().is_none_or(|a| {
                    a.iter().any(|s| {
                        !matches!(
                            s.as_str(),
                            Some("Classical.choice" | "Quot.sound" | "propext")
                        )
                    })
                })
            }) {
                return Err("unexpected_lean_axioms".into());
            }
            let audit = "target/self-check-proof/VerdictsAudit.lean";
            if self.file(audit, false)?.0 != report["audit_sha256"] {
                return Err("lean_audit_digest_mismatch".into());
            }
            self.evidence.reason = "inspected_lean_component_report_not_a_fresh_kernel_run".into();
            Ok(Verdict::Success)
        }
    }
}

fn source_paths(root: &Path, nibli: bool) -> Result<BTreeSet<String>> {
    fn walk(
        root: &Path,
        relative: &str,
        extensions: &[&str],
        recursive: bool,
        out: &mut BTreeSet<String>,
    ) -> Result<()> {
        let dir = path(root, relative)?;
        for entry in std::fs::read_dir(dir).map_err(|_| "source_inventory_unavailable")? {
            let entry = entry.map_err(|_| "source_inventory_unavailable")?;
            let kind = entry
                .file_type()
                .map_err(|_| "source_inventory_unavailable")?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "target" {
                continue;
            }
            let relative = format!("{relative}/{name}");
            if kind.is_symlink() {
                return Err("source_inventory_contains_symlink".into());
            }
            if kind.is_dir() && recursive {
                walk(root, &relative, extensions, true, out)?;
            } else if kind.is_file()
                && Path::new(&name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| extensions.contains(&e))
            {
                out.insert(relative);
            }
            if out.len() > 4096 {
                return Err("source_inventory_exceeds_bound".into());
            }
        }
        Ok(())
    }
    let mut out: BTreeSet<String> = [
        "Cargo.lock",
        "Cargo.toml",
        "nibli-verify/spec/nibli_checker.nibli",
        "proofs/self-check/lean-toolchain",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    walk(
        root,
        "proofs/self-check",
        &["py", "lean", "toml", "nix", "lock", "json"],
        false,
        &mut out,
    )?;
    for name in CRATES {
        out.insert(format!("{name}/Cargo.toml"));
        walk(
            root,
            &format!("{name}/src"),
            &["rs", "pest"],
            true,
            &mut out,
        )?;
        if root.join(name).join("build.rs").is_file() {
            out.insert(format!("{name}/build.rs"));
        }
    }
    if nibli {
        out.extend(["flake.nix".into(), "flake.lock".into()]);
        for base in ["nibli-verify", "proofs/self-check/llbc-json"] {
            walk(root, base, &["rs", "toml", "lock", "nibli"], true, &mut out)?;
        }
    }
    Ok(out)
}

fn inspect(store: &Store, inspector: &mut Inspector<'_>, check: &Check) -> Result<Verdict> {
    match check {
        Check::FileExists { path: name } => {
            let path = path(inspector.root, name)?;
            match std::fs::metadata(path) {
                Ok(m) if m.is_file() => {
                    inspector.file(name, false)?;
                    Ok(Verdict::Success)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    inspector.evidence.missing_files.push(name.clone());
                    Ok(Verdict::Failure)
                }
                _ => Err("file_type_or_access_unknown".into()),
            }
        }
        Check::FileSha256 { path, sha256 } => Ok(if &inspector.file(path, false)?.0 == sha256 {
            Verdict::Success
        } else {
            Verdict::Failure
        }),
        Check::JsonEquals {
            path,
            pointer,
            expected,
        } => Ok(
            if inspector.json(path)?.pointer(pointer) == Some(expected) {
                Verdict::Success
            } else {
                Verdict::Failure
            },
        ),
        Check::Record {
            origin,
            record,
            sha256,
        } => {
            let key = json!([origin, record]).to_string();
            let found = store
                .all()?
                .into_iter()
                .find(|r| r.origin == *origin && r.payload["id"] == *record);
            if let Some(found) = found {
                let actual = hash(&found);
                inspector.evidence.records.insert(key, actual.clone());
                Ok(if &actual == sha256 {
                    Verdict::Success
                } else {
                    Verdict::Failure
                })
            } else {
                inspector.evidence.missing_records.push(key);
                Ok(Verdict::Failure)
            }
        }
        Check::NibliReport { path } => inspector.report(path, true),
        Check::LeanReport { path } => inspector.report(path, false),
        Check::Reported { .. } => {
            inspector.evidence.class = EvidenceClass::Reported;
            inspector.evidence.reason =
                "manual_requirement_has_no_independently_inspected_evidence".into();
            Ok(Verdict::Inconclusive)
        }
    }
}

pub fn check(store: &Store, workspace: &Path, task: &str, criterion: &str) -> Result<Entry> {
    check_with_clock(store, workspace, task, criterion, None)
}

pub(crate) fn check_with_clock(
    store: &Store,
    workspace: &Path,
    task: &str,
    criterion: &str,
    clock: Option<&super::runtime::ActiveClock>,
) -> Result<Entry> {
    let _span = super::profile::span("task_check");
    let started = Instant::now();
    let mut history = History::read(store)?;
    let remaining = history
        .available(task)?
        .min(clock.map_or(u64::MAX, |c| c.remaining_ms()));
    if remaining == 0 {
        return Err("task active-time budget exhausted".into());
    }
    let definition = history.task(task)?;
    same_workspace(definition, workspace)?;
    let criterion = definition
        .definition
        .criteria
        .iter()
        .find(|c| c.id == criterion)
        .ok_or("unknown task criterion")?;
    let mut inspector = Inspector::new(workspace, remaining);
    let mut verdict = match inspect(store, &mut inspector, &criterion.check) {
        Ok(verdict) => verdict,
        Err(reason) => {
            inspector.evidence.reason = reason;
            inspector.evidence.replay_available = false;
            Verdict::Inconclusive
        }
    };
    if clock.is_some_and(|c| c.remaining_ms() == 0) {
        verdict = Verdict::Inconclusive;
        inspector.evidence.reason = "active_budget_exhausted".into();
    }
    let criterion = criterion.id.clone();
    super::journal::append(
        store,
        &mut history,
        task,
        Event::Checked {
            criterion,
            verdict,
            evidence: inspector.evidence,
            active_ms: started.elapsed().as_millis() as u64,
        },
    )
}

pub(crate) fn fresh(
    store: &Store,
    workspace: &Path,
    check: &Check,
    evidence: &Evidence,
    remaining_ms: u64,
) -> bool {
    let mut inspector = Inspector::new(workspace, remaining_ms.min(30_000));
    // Rebuild proof source inventories as well as rehashing recorded files: a
    // newly added source is a change even when every old digest still matches.
    if matches!(check, Check::NibliReport { .. } | Check::LeanReport { .. }) {
        if inspect(store, &mut inspector, check).is_err() {
            return false;
        }
        return inspector.evidence == *evidence;
    }
    if evidence.files.iter().any(|(p, h)| {
        inspector
            .file(p, false)
            .map_or(true, |(actual, _)| actual != *h)
    }) {
        return false;
    }
    if evidence.missing_files.iter().any(|p| {
        path(workspace, p).map_or(
            true,
            |p| !matches!(std::fs::metadata(p),Err(e) if e.kind()==std::io::ErrorKind::NotFound),
        )
    }) {
        return false;
    }
    let Ok(records) = store.all() else {
        return false;
    };
    let actual: BTreeMap<_, _> = records
        .iter()
        .map(|r| (json!([r.origin, r.payload["id"]]).to_string(), hash(r)))
        .collect();
    evidence
        .records
        .iter()
        .all(|(key, h)| actual.get(key) == Some(h))
        && evidence
            .missing_records
            .iter()
            .all(|key| !actual.contains_key(key))
}

/// Registered, read-only templates. These return data to the planner and cannot
/// launch a command, enlarge a budget or alter any approval.
pub fn inspect_inputs(
    store: &Store,
    workspace: &Path,
    task: &Task,
    kind: LessonKind,
) -> Result<Value> {
    inspect_inputs_scoped(store, workspace, task, kind, None)
}

pub(crate) fn inspect_inputs_scoped(
    store: &Store,
    workspace: &Path,
    task: &Task,
    kind: LessonKind,
    allowed: Option<&std::collections::BTreeSet<String>>,
) -> Result<Value> {
    same_workspace(task, workspace)?;
    let mut rows = Vec::new();
    for criterion in &task.definition.criteria {
        match &criterion.check {
            Check::Record { origin, record, .. } if kind == LessonKind::RetrieveSources => {
                let found = store.all()?.into_iter().find(|r| {
                    r.origin == *origin
                        && r.payload["id"] == *record
                        && allowed.is_none_or(|keys| keys.contains(&r.key()))
                });
                rows.push(json!({"criterion":criterion.id,"origin":origin,"record":record,"found":found.is_some(),
                    "excerpt":found.map(|r|r.payload["text"].as_str().unwrap_or("").chars().take(2000).collect::<String>())}));
            }
            Check::FileExists { path: name }
            | Check::FileSha256 { path: name, .. }
            | Check::JsonEquals { path: name, .. }
            | Check::NibliReport { path: name }
            | Check::LeanReport { path: name }
                if kind != LessonKind::RetrieveSources =>
            {
                let file = path(workspace, name)?;
                let metadata = std::fs::metadata(&file).ok();
                rows.push(json!({"criterion":criterion.id,"exists":metadata.is_some(),"bytes":metadata.map(|m|m.len()),
                    "parent_exists":file.parent().is_some_and(|p|p.is_dir())}));
            }
            _ => {}
        }
    }
    Ok(
        json!({"template":kind,"observations":rows,"meaning":"Bounded input/source/size inspection, not a task conclusion or command authorization."}),
    )
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn inspected_report_controls_detect_added_sources_and_missing_theorems() {
        // This synthetic manifest tests inspection and attribution, not Lean.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for directory in [
            "proofs/self-check",
            "nibli-verify/spec",
            "target/self-check-proof",
        ] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        for name in CRATES {
            std::fs::create_dir_all(root.join(name).join("src")).unwrap();
        }
        for name in source_paths(root, false).unwrap() {
            std::fs::write(root.join(name), "synthetic fixture source").unwrap();
        }
        let sources: BTreeMap<_, _> = source_paths(root, false)
            .unwrap()
            .into_iter()
            .map(|name| {
                let hash = digest(&std::fs::read(root.join(&name)).unwrap());
                (name, hash)
            })
            .collect();
        for name in [
            "input.llbc",
            "artifact.lean",
            "log.txt",
            "target/self-check-proof/VerdictsAudit.lean",
        ] {
            std::fs::write(
                root.join(name),
                "synthetic inspection fixture, not a proof execution",
            )
            .unwrap();
        }
        let sha = digest(b"synthetic inspection fixture, not a proof execution");
        let mut report = json!({"schema":1,"component":"verdict_algebra","self_check_equivalence_verified":false,
            "component_verified":true,"sources":sources,"extraction":{"status":"translated","llbc_file":"input.llbc",
                "llbc_sha256":sha,"artifacts":{"artifact.lean":sha}},"audit_sha256":sha});
        for name in ["build", "proof", "audit", "lean-version"] {
            report[name] = json!({"status":"ok","exit_code":0,"log":"log.txt"});
        }
        report["axioms"] = json!(
            LEAN_THEOREMS
                .iter()
                .map(|name| (format!("NibliSelfCheck.{name}"), Vec::<String>::new()))
                .collect::<BTreeMap<_, _>>()
        );
        std::fs::write(root.join("report.json"), report.to_string()).unwrap();
        let store = Store::open(&root.join("memory")).unwrap();
        let kind = Check::LeanReport {
            path: "report.json".into(),
        };
        let mut inspected = Inspector::new(root, 30_000);
        assert_eq!(
            inspect(&store, &mut inspected, &kind).unwrap(),
            Verdict::Success
        );
        assert_eq!(inspected.evidence.class, EvidenceClass::Inspected);
        assert!(inspected.evidence.reason.contains("not_a_fresh_kernel_run"));
        assert!(fresh(&store, root, &kind, &inspected.evidence, 30_000));
        let added = root.join("nibli-types/src/new.rs");
        std::fs::write(&added, "new source").unwrap();
        assert!(!fresh(&store, root, &kind, &inspected.evidence, 30_000));
        std::fs::remove_file(added).unwrap();
        assert!(fresh(&store, root, &kind, &inspected.evidence, 30_000));
        let axioms = report["axioms"].as_object_mut().unwrap();
        axioms.remove("NibliSelfCheck.negation_total");
        axioms.insert("unrelated_theorem".into(), json!([]));
        std::fs::write(root.join("report.json"), report.to_string()).unwrap();
        assert!(
            inspect(&store, &mut Inspector::new(root, 30_000), &kind)
                .unwrap_err()
                .contains("theorem_inventory")
        );
    }
}
