//! Activation requires a complete independent study, not an agent's claim of
//! improvement. Trial originals stay in an isolated private directory.
use super::*;
use crate::store::{Record, Store, digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ACTIVATIONS: &str = "lucy:task-policy:v1";
pub const ARMS: [&str; 4] = [
    "full",
    "lessons_withheld",
    "memory_removed",
    "roadmap_removed",
];

/// Activation is confined to the complete tested scope, including workspace.
pub fn policy_key(scope: &Scope) -> String {
    hash(&json!({"scope":scope,
        "checker":digest(SPEC.as_bytes()),"policy":POLICY}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trial {
    pub case: String,
    pub disguise: String,
    pub arm: String,
    pub scope: Scope,
    pub criteria_digest: String,
    pub inputs_digest: String,
    pub budget: Budget,
    pub completed: bool,
    pub false_verified: bool,
    pub budget_violation: bool,
    pub inconclusive: bool,
    pub probability_ppm: u32,
    pub observed: Option<bool>,
    pub active_ms: u64,
    pub checking_ms: u64,
    pub task: String,
    pub journal_digest: String,
    pub oracle_digest: String,
    pub implementation: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub schema: u32,
    pub protocol: String,
    pub implementation: String,
    pub model: String,
    pub evaluator: String,
    pub evaluator_model: String,
    pub bundle_digest: String,
    pub live: bool,
    pub complete: bool,
    pub active_ms: u64,
    pub calibration: BTreeMap<String, u32>,
    pub trials: Vec<Trial>,
}

fn quantile(values: &[f64], p: f64) -> f64 {
    let pos = (values.len() - 1) as f64 * p;
    let low = pos.floor() as usize;
    values[low] + (values[pos.ceil() as usize] - values[low]) * (pos - low as f64)
}

fn interval(values: &[f64]) -> [f64; 2] {
    let mut seed = 6u64;
    let mut samples = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let mut sum = 0.;
        for _ in values {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            sum += values[(seed % values.len() as u64) as usize];
        }
        samples.push(sum / values.len() as f64);
    }
    samples.sort_by(f64::total_cmp);
    [quantile(&samples, 0.025), quantile(&samples, 0.975)]
}

pub fn score(study: &Study) -> Result<Value> {
    if study.schema != 2
        || study.protocol != digest(PROTOCOL.as_bytes())
        || !is_digest(&study.implementation)
        || !is_digest(&study.bundle_digest)
        || !bounded(&study.model)
        || !bounded(&study.evaluator)
        || !bounded(&study.evaluator_model)
    {
        return Err("invalid study identity or protocol".into());
    }
    let mut grouped: BTreeMap<String, Vec<&Trial>> = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for trial in &study.trials {
        if !ARMS.contains(&trial.arm.as_str())
            || !["a", "b"].contains(&trial.disguise.as_str())
            || !bounded(&trial.case)
            || trial.model != study.model
            || trial.scope.model != study.model
            || trial.implementation != study.implementation
            || trial.probability_ppm > 1_000_000
            || [
                &trial.criteria_digest,
                &trial.inputs_digest,
                &trial.journal_digest,
                &trial.oracle_digest,
            ]
            .iter()
            .any(|s| !is_digest(s))
        {
            return Err("invalid trial identity, evidence or frozen model".into());
        }
        let scope = policy_key(&trial.scope);
        if !identities.insert((
            scope.clone(),
            trial.case.clone(),
            trial.arm.clone(),
            trial.disguise.clone(),
        )) {
            return Err("duplicate held-out trial".into());
        }
        grouped.entry(scope).or_default().push(trial);
    }
    let families: BTreeSet<_> = study
        .trials
        .iter()
        .map(|t| t.scope.family.as_str())
        .collect();
    let base_eligible = study.live
        && study.complete
        && study.active_ms <= 3_600_000
        && !study.model.contains("revision-unknown")
        && grouped.len() == 2
        && study.calibration.len() == 2
        && grouped
            .keys()
            .all(|key| study.calibration.get(key) == Some(&10))
        && families == BTreeSet::from(["verification", "retrieval"]);
    let mut scopes = Vec::new();
    for (scope, trials) in grouped {
        let cases: BTreeSet<_> = trials.iter().map(|t| &t.case).collect();
        let mut valid = cases.len() == 20
            && study.calibration.get(&scope) == Some(&10)
            && trials.len() == 20 * 2 * 4;
        let mut differences = Vec::new();
        for case in &cases {
            let mut difference = 0.;
            for disguise in ["a", "b"] {
                let rows: Vec<_> = trials
                    .iter()
                    .filter(|t| &t.case == *case && t.disguise == disguise)
                    .copied()
                    .collect();
                valid &= rows.len() == 4;
                if let Some(first) = rows.first() {
                    valid &= rows.iter().all(|t| {
                        t.criteria_digest == first.criteria_digest && t.budget == first.budget
                    });
                }
                let full = rows.iter().find(|t| t.arm == "full");
                let baseline = rows.iter().find(|t| t.arm == "lessons_withheld");
                if let (Some(full), Some(baseline)) = (full, baseline) {
                    valid &= full.inputs_digest == baseline.inputs_digest;
                    difference += (u8::from(full.completed) as f64
                        - u8::from(baseline.completed) as f64)
                        / 2.;
                } else {
                    valid = false;
                }
            }
            differences.push(difference);
        }
        let delta = if differences.is_empty() {
            0.
        } else {
            differences.iter().sum::<f64>() / differences.len() as f64
        };
        let ci = if differences.is_empty() {
            [0., 0.]
        } else {
            interval(&differences)
        };
        let mut arms = Vec::new();
        for arm in ARMS {
            let rows: Vec<_> = trials.iter().filter(|t| t.arm == arm).copied().collect();
            let mut summary = metrics(&rows);
            summary["arm"] = json!(arm);
            summary["by_disguise"] = json!(["a", "b"].map(|disguise| {
                let mut value = metrics(
                    &rows
                        .iter()
                        .filter(|t| t.disguise == disguise)
                        .copied()
                        .collect::<Vec<_>>(),
                );
                value["disguise"] = json!(disguise);
                value
            }));
            arms.push(summary);
        }
        let safe = trials
            .iter()
            .all(|t| !t.false_verified && !t.budget_violation);
        let full_metrics = metrics(
            &trials
                .iter()
                .filter(|t| t.arm == "full")
                .copied()
                .collect::<Vec<_>>(),
        );
        let forecast_qualifies = full_metrics["brier"].as_f64().is_some_and(|b| b < 0.25);
        scopes.push(json!({"scope":scope,"family":trials[0].scope.family,"complete":valid,"completion_delta":valid.then_some(delta),
            "forecast_qualifies":forecast_qualifies,
            "paired_ci95":valid.then_some(ci),"eligible":base_eligible&&valid&&safe&&delta>=0.10&&ci[0]>0.,"arms":arms}));
    }
    if scopes.iter().any(|s| s["complete"] != true)
        || study
            .trials
            .iter()
            .any(|t| t.false_verified || t.budget_violation)
    {
        for scope in &mut scopes {
            scope["eligible"] = json!(false);
        }
    }
    Ok(
        json!({"schema":2,"protocol":study.protocol,"implementation":study.implementation,"model":study.model,
        "policy":POLICY,"study_digest":hash(study),"complete":study.complete,"live":study.live,
        "active_ms":study.active_ms,"scopes":scopes,
        "s6a_complete":false,"meaning":"Exact-scope policy comparison; model-change and independent adjudication requirements remain separate."}),
    )
}

fn metrics(rows: &[&Trial]) -> Value {
    let scores: Vec<_> = rows
        .iter()
        .filter_map(|t| {
            t.observed.map(|outcome| {
                let p = t.probability_ppm as f64 / 1_000_000.;
                (p - f64::from(outcome)).powi(2)
            })
        })
        .collect();
    json!({"trials":rows.len(),"completed":rows.iter().filter(|t|t.completed).count(),
        "false_verified":rows.iter().filter(|t|t.false_verified).count(),"inconclusive":rows.iter().filter(|t|t.inconclusive).count(),
        "budget_violations":rows.iter().filter(|t|t.budget_violation).count(),
        "active_ms":rows.iter().map(|t|t.active_ms as u128).sum::<u128>(),
        "checking_ms":rows.iter().map(|t|t.checking_ms as u128).sum::<u128>(),
        "brier":(!scores.is_empty()).then(||scores.iter().sum::<f64>()/scores.len() as f64),
        "scored":scores.len(),"excluded":rows.len()-scores.len(),"cold_brier":0.25,
        "tokens":Value::Null,"cpu_ms":Value::Null,"peak_memory_bytes":Value::Null})
}

pub fn isolated(root: &Path, workspace: &Path) -> Result<()> {
    let root = root
        .canonicalize()
        .map_err(|_| "study root must already exist")?;
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "study workspace must already exist")?;
    let home = std::env::var_os("LUCY_HOME")
        .map(PathBuf::from)
        .ok_or("study requires isolated LUCY_HOME")?
        .canonicalize()
        .map_err(|_| "isolated home missing")?;
    let data = std::env::var_os("LUCY_DATA_DIR")
        .map(PathBuf::from)
        .ok_or("study requires isolated LUCY_DATA_DIR")?
        .canonicalize()
        .map_err(|_| "isolated data missing")?;
    let base = std::env::var_os("LUCY_STUDY_ROOT")
        .map(PathBuf::from)
        .ok_or("study requires runner-owned root")?
        .canonicalize()
        .map_err(|_| "study base missing")?;
    let real = std::env::var_os("LUCY_STUDY_REAL_DATA")
        .map(PathBuf::from)
        .ok_or("study requires real-data exclusion")?;
    let real = real
        .canonicalize()
        .or_else(|_| std::path::absolute(real))
        .map_err(|e| e.to_string())?;
    let user_home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME missing")?);
    let default_data = user_home.join(".local/share/lucy");
    if root != data
        || !root.starts_with(&base)
        || !home.starts_with(&base)
        || !workspace.starts_with(&base)
        || root == base
        || home == base
        || root.starts_with(&home)
        || home.starts_with(&root)
        || home.starts_with(&workspace)
        || workspace.starts_with(&home)
        || root.starts_with(&workspace)
        || workspace.starts_with(&root)
        || base.starts_with(&real)
        || real.starts_with(&base)
        || base.starts_with(&default_data)
        || default_data.starts_with(&base)
        || base == user_home
        || !base.join(".lucy-isolated-study").is_file()
        || base.ancestors().any(|p| p.join(".git").exists())
        || base.starts_with("/nix/store")
    {
        return Err("study refuses real, overlapping or repository-backed state".into());
    }
    Ok(())
}

pub(crate) fn implementation() -> Result<String> {
    static ID: std::sync::OnceLock<Result<String>> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut file = std::fs::File::open(std::env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = [0; 65536];
        loop {
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })
    .clone()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialRequest {
    pub case: String,
    pub disguise: String,
    pub arm: String,
    pub prompt: String,
    pub definition: Definition,
    pub oracle: Vec<Criterion>,
    pub inputs_digest: String,
    #[serde(default)]
    pub approvals: Vec<Value>,
    #[serde(default)]
    pub output_writes: Vec<OutputWrite>,
    #[serde(default)]
    pub recall_records: Vec<Record>,
    #[serde(default)]
    pub archive: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputWrite {
    pub path: String,
    pub max_bytes: usize,
}

impl TrialRequest {
    pub fn allows(&self, proposal: &Value) -> bool {
        if proposal["tool"] == "write" {
            let args = &proposal["arguments"];
            return args.as_object().is_some_and(|o| o.len() == 2)
                && self.output_writes.iter().any(|rule| {
                    args["path"] == rule.path
                        && args["text"]
                            .as_str()
                            .is_some_and(|s| s.len() <= rule.max_bytes)
                });
        }
        proposal["tool"] == "command"
            && self
                .approvals
                .iter()
                .any(|p| p["tool"] == "command" && p["arguments"] == proposal["arguments"])
    }
}

pub fn trial(root: &Path, workspace: &Path, input: &str) -> Result<Value> {
    isolated(root, workspace)?;
    let request: TrialRequest = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if !ARMS.contains(&request.arm.as_str()) && request.arm != "calibration" {
        return Err("unknown study arm".into());
    }
    if request.oracle.is_empty() || !is_digest(&request.inputs_digest) {
        return Err("independent oracle and input digest required".into());
    }
    for rule in &request.output_writes {
        checks::validate(&Check::FileExists {
            path: rule.path.clone(),
        })?;
        if rule.max_bytes == 0 || rule.max_bytes > 1024 * 1024 {
            return Err("study output writes must be bounded to 1 MiB".into());
        }
    }
    for record in &request.recall_records {
        record.validate()?;
        if !record.origin.starts_with("fixture:") || record.payload["private"] != true {
            return Err("only private fixture records may enter study recall".into());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = request;
        Err("native study trials require Unix".into())
    }
    #[cfg(unix)]
    {
        let started = std::time::Instant::now();
        let archive = study_artifacts::prepare(workspace, &request)?;
        let service = crate::desktop::Service::for_study(
            root.to_owned(),
            workspace,
            matches!(
                request.arm.as_str(),
                "full" | "memory_removed" | "roadmap_removed"
            ),
        )?;
        let (task, response) = service.study_chat(workspace, &request)?;
        let native_active_ms = started.elapsed().as_millis() as u64;
        let store = Store::open(root)?;
        let history = History::read(&store)?;
        let frozen = history.task(&task)?;
        let mut oracle = request.definition.clone();
        oracle.source = "independent-study-oracle".into();
        oracle.session = identity("oracle")?;
        oracle.criteria = request.oracle.clone();
        oracle.budget = Budget::default();
        let oracle_task = begin(&store, workspace, oracle, Observer::Cli)?;
        for criterion in &request.oracle {
            check(&store, workspace, &oracle_task.id, &criterion.id)?;
        }
        let oracle_outcome = finish(&store, workspace, &oracle_task.id, false)?;
        let oracle_pass = matches!(
            oracle_outcome.event,
            Event::Finished {
                verdict: Verdict::Success,
                ..
            }
        );
        let result = history.outcome(&task).and_then(|e| {
            if let Event::Finished { verdict, .. } = e.event {
                Some(verdict)
            } else {
                None
            }
        });
        let completed = oracle_pass && result == Some(Verdict::Success);
        let checking_ms = history
            .rows(&task)
            .iter()
            .map(|e| match e.event {
                Event::Checked { active_ms, .. } | Event::Finished { active_ms, .. } => active_ms,
                _ => 0,
            })
            .sum();
        let usage = history.usage(&task);
        let artifacts = study_artifacts::finish(workspace, &archive, &request, &store)?;
        let trial = Trial {
            case: request.case,
            disguise: request.disguise,
            arm: request.arm,
            scope: frozen.scope.clone(),
            criteria_digest: hash(&request.oracle),
            inputs_digest: request.inputs_digest,
            budget: frozen.definition.budget,
            completed,
            false_verified: result == Some(Verdict::Success) && !oracle_pass,
            budget_violation: usage.steps > frozen.definition.budget.steps
                || usage.active_ms > frozen.definition.budget.active_ms
                || native_active_ms > frozen.definition.budget.active_ms,
            inconclusive: result.is_none_or(|v| v == Verdict::Inconclusive),
            probability_ppm: frozen.probability_ppm,
            observed: result.and_then(|v| match v {
                Verdict::Success | Verdict::Failure => Some(completed),
                _ => None,
            }),
            active_ms: started.elapsed().as_millis() as u64,
            checking_ms,
            task: task.clone(),
            journal_digest: hash(&history.entries),
            oracle_digest: hash(&oracle_outcome),
            implementation: implementation()?,
            model: frozen.scope.model.clone(),
        };
        let all_history = History::read(&store)?;
        let trace: Value = serde_json::from_slice(
            &std::fs::read(root.join("visible-trace.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if !trace
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["kind"] == "request"))
        {
            return Err("study model-input evidence missing".into());
        }
        let originals = json!({"schema":2,"trial":trial,"native_active_ms":native_active_ms,"journal":history.entries,"oracle":oracle_outcome,
            "oracle_journal":all_history.rows(&oracle_task.id),"response":response,
            "artifacts":artifacts,"visible_trace":trace});
        crate::store::save(
            &root.join("trial.json"),
            &serde_json::to_vec(&originals).map_err(|e| e.to_string())?,
        )?;
        Ok(json!({"trial":trial,"originals_digest":hash(&originals)}))
    }
}

pub fn score_command(root: &Path, input: &str) -> Result<Value> {
    let started = std::time::Instant::now();
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        study: Study,
        originals: Vec<String>,
        calibration_originals: Vec<String>,
    }
    let input: Input = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if input.originals.len() != input.study.trials.len() {
        return Err("study needs every private trial original".into());
    }
    let mut all_trials: Vec<_> = input
        .study
        .trials
        .iter()
        .cloned()
        .zip(input.originals.iter().cloned())
        .collect();
    let mut counts = BTreeMap::new();
    let mut cases = BTreeSet::new();
    for file in &input.calibration_originals {
        let original = read_original(file)?;
        let trial: Trial = serde_json::from_value(original["trial"].clone())
            .map_err(|_| "invalid calibration original")?;
        if trial.arm != "calibration"
            || trial.disguise != "a"
            || trial.implementation != input.study.implementation
            || trial.model != input.study.model
            || !cases.insert(trial.case.clone())
            || input.study.trials.iter().any(|t| t.case == trial.case)
        {
            return Err("calibration identity or separation invalid".into());
        }
        *counts.entry(policy_key(&trial.scope)).or_insert(0u32) += 1;
        all_trials.push((trial, file.clone()));
    }
    if counts != input.study.calibration {
        return Err("calibration counts require preserved original receipts".into());
    }
    if all_trials
        .iter()
        .map(|(t, _)| t.active_ms as u128)
        .sum::<u128>()
        > input.study.active_ms as u128
    {
        return Err("study duration is less than its observed trials".into());
    }
    for (trial, file) in &all_trials {
        let original = read_original(file)?;
        let entries: BTreeMap<String, Entry> = serde_json::from_value(original["journal"].clone())
            .map_err(|_| "missing trial journal")?;
        let history = History { entries };
        history.audit()?;
        if hash(&history.entries) != trial.journal_digest
            || hash(&original["oracle"]) != trial.oracle_digest
            || original["trial"] != json!(trial)
        {
            return Err("trial provenance mismatch".into());
        }
        let task = history.task(&trial.task)?;
        let result = history.outcome(&trial.task).and_then(|e| match e.event {
            Event::Finished { verdict, .. } => Some(verdict),
            _ => None,
        });
        let oracle: Entry = serde_json::from_value(original["oracle"].clone())
            .map_err(|_| "invalid oracle receipt")?;
        let oracle_rows: Vec<Entry> = serde_json::from_value(original["oracle_journal"].clone())
            .map_err(|_| "missing oracle history")?;
        let mut all = history.clone();
        for entry in oracle_rows {
            all.entries.insert(entry.id.clone(), entry);
        }
        all.audit()?;
        if all.outcome(&oracle.task) != Some(&oracle)
            || hash(&all.task(&oracle.task)?.definition.criteria) != trial.criteria_digest
            || original["artifacts"]["oracle"] != json!(all.task(&oracle.task)?.definition.criteria)
        {
            return Err("oracle does not follow its frozen criteria and receipts".into());
        }
        let oracle_pass = matches!(
            oracle.event,
            Event::Finished {
                verdict: Verdict::Success,
                ..
            }
        );
        if original["schema"] != 2
            || study_artifacts::replay(&original["artifacts"])? != oracle_pass
        {
            return Err("preserved artifacts disagree with the oracle receipt".into());
        }
        let completed = oracle_pass && result == Some(Verdict::Success);
        let usage = history.usage(&trial.task);
        let native_active_ms = original["native_active_ms"]
            .as_u64()
            .ok_or("native elapsed time unavailable")?;
        let checking_ms: u64 = history
            .rows(&trial.task)
            .iter()
            .map(|e| match e.event {
                Event::Checked { active_ms, .. } | Event::Finished { active_ms, .. } => active_ms,
                _ => 0,
            })
            .sum();
        if trial.scope != task.scope
            || trial.scope.observer != Observer::Native
            || trial.checking_ms != checking_ms
            || trial.active_ms < usage.active_ms
            || native_active_ms > trial.active_ms
            || native_active_ms < usage.active_ms
            || trial.probability_ppm != task.probability_ppm
            || trial.budget != task.definition.budget
            || trial.completed != completed
            || trial.false_verified != (result == Some(Verdict::Success) && !oracle_pass)
            || trial.inconclusive != result.is_none_or(|v| v == Verdict::Inconclusive)
            || trial.budget_violation
                != (usage.steps > task.definition.budget.steps
                    || usage.active_ms > task.definition.budget.active_ms
                    || native_active_ms > task.definition.budget.active_ms)
            || trial.observed
                != result.and_then(|v| match v {
                    Verdict::Success | Verdict::Failure => Some(completed),
                    _ => None,
                })
        {
            return Err("study metrics disagree with original receipts".into());
        }
    }
    let store = Store::open(root)?;
    let mut report = score(&input.study)?;
    let active_ms = input
        .study
        .active_ms
        .saturating_add(started.elapsed().as_millis() as u64);
    report["active_ms"] = json!(active_ms);
    report["adjudication_ms"] = json!(started.elapsed().as_millis() as u64);
    let calibration_safe = all_trials.iter().all(|(trial, _)| {
        trial.arm != "calibration" || (!trial.false_verified && !trial.budget_violation)
    });
    report["calibration_safe"] = json!(calibration_safe);
    if active_ms > 3_600_000 {
        report["complete"] = json!(false);
    }
    if active_ms > 3_600_000 || !calibration_safe {
        for scope in report["scopes"].as_array_mut().unwrap() {
            scope["eligible"] = json!(false);
        }
    }
    let record = Record {
        origin: "lucy:task-study:v1".into(),
        payload: json!({"id":hash(&report),"private":true,
        "channel":"task_study","text":"Reviewed aggregate task study; held-out originals remain isolated.","report":report}),
    };
    store.record(&record)?;
    if !store.all()?.iter().any(|r| r == &record) {
        return Err("study report readback failed".into());
    }
    Ok(report)
}

fn read_original(file: &str) -> Result<Value> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|_| "trial original unavailable")?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "trial original unreadable")?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("trial original exceeds bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid trial original".into())
}

pub fn enabled(store: &Store, scope: &Scope) -> Result<bool> {
    let key = policy_key(scope);
    let all = store.all()?;
    let mut records: Vec<_> = all
        .iter()
        .filter(|r| r.origin == ACTIVATIONS && r.payload["scope"] == key)
        .collect();
    records.sort_by(|a, b| {
        a.payload["sequence"]
            .as_u64()
            .cmp(&b.payload["sequence"].as_u64())
    });
    let implementation = implementation()?;
    Ok(records.last().is_some_and(|r| {
        r.payload["private"] == true
            && r.payload["enabled"] == true
            && r.payload["checker"] == digest(SPEC.as_bytes())
            && r.payload["policy"] == POLICY
            && r.payload["implementation"] == implementation
            && all.iter().any(|s| {
                s.origin == "lucy:task-study:v1"
                    && s.payload["private"] == true
                    && s.payload["id"] == r.payload["study"]
                    && s.payload["id"] == hash(&s.payload["report"])
                    && s.payload["report"]["protocol"] == digest(PROTOCOL.as_bytes())
                    && s.payload["report"]["implementation"] == implementation
                    && s.payload["report"]["scopes"]
                        .as_array()
                        .is_some_and(|rows| {
                            rows.iter()
                                .any(|row| row["scope"] == key && row["eligible"] == true)
                        })
            })
    }))
}

pub fn policy_command(store: &Store, input: &str) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        scope: String,
        enabled: bool,
        study: Option<String>,
    }
    let input: Input = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if !is_digest(&input.scope) {
        return Err("policy requires a tested scope digest".into());
    }
    let records = store.all()?;
    if input.enabled {
        let id = input
            .study
            .as_ref()
            .ok_or("activation requires a scored study ID")?;
        let report = records
            .iter()
            .find(|r| r.origin == "lucy:task-study:v1" && r.payload["id"] == *id)
            .ok_or("scored study unavailable")?;
        if report.payload["private"] != true
            || report.payload["id"] != hash(&report.payload["report"])
            || report.payload["report"]["protocol"] != digest(PROTOCOL.as_bytes())
            || report.payload["report"]["implementation"] != implementation()?
            || !report.payload["report"]["scopes"]
                .as_array()
                .is_some_and(|a| {
                    a.iter()
                        .any(|s| s["scope"] == input.scope && s["eligible"] == true)
                })
        {
            return Err(
                "measured-benefit gate has not passed for this implementation and scope".into(),
            );
        }
    }
    let sequence = records
        .iter()
        .filter(|r| r.origin == ACTIVATIONS)
        .filter_map(|r| r.payload["sequence"].as_u64())
        .max()
        .unwrap_or(0)
        + 1;
    let payload = json!({"id":identity("policy")?,"private":true,"channel":"task_policy","text":"Task adaptation setting with explicit evidence.",
        "scope":input.scope,"enabled":input.enabled,"study":input.study,"sequence":sequence,"checker":digest(SPEC.as_bytes()),"policy":POLICY,"implementation":implementation()?});
    store.record(&Record {
        origin: ACTIVATIONS.into(),
        payload: payload.clone(),
    })?;
    Ok(payload)
}
