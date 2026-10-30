use super::*;
use crate::store::{Record, Store, digest};
use nibli_engine::NibliEngine;
use nibli_types::logic::QueryResult;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Default)]
pub struct History {
    pub entries: BTreeMap<String, Entry>,
}

fn constant(text: &str) -> String {
    let mut out = String::from("C");
    for byte in text.bytes() {
        use std::fmt::Write;
        write!(out, "{byte:02x}").unwrap();
    }
    out
}

pub(crate) fn validate_definition(definition: &Definition) -> Result<()> {
    if [
        &definition.source,
        &definition.session,
        &definition.title,
        &definition.family,
        &definition.method,
        &definition.model,
        &definition.environment,
    ]
    .iter()
    .any(|s| !bounded(s))
        || definition.criteria.is_empty()
        || definition.criteria.len() > 12
        || definition.budget.steps == 0
        || definition.budget.steps > 12
        || definition.budget.active_ms == 0
        || definition.budget.active_ms > Budget::default().active_ms
    {
        return Err(
            "invalid task definition or budget (1..12 steps; at most 24 active minutes)".into(),
        );
    }
    let mut ids = BTreeSet::new();
    for criterion in &definition.criteria {
        if !bounded(&criterion.id) || !bounded(&criterion.description) || !ids.insert(&criterion.id)
        {
            return Err("criteria need unique bounded IDs and descriptions".into());
        }
        checks::validate(&criterion.check)?;
    }
    Ok(())
}

impl History {
    pub fn read(store: &Store) -> Result<Self> {
        let _span = profile::span("history_read");
        let mut history = Self::default();
        for record in store.all()?.into_iter().filter(|r| r.origin == ORIGIN) {
            let entry: Entry = serde_json::from_value(record.payload["entry"].clone())
                .map_err(|e| format!("invalid task journal: {e}"))?;
            if record.payload["id"] != entry.id
                || record.payload["private"] != true
                || history.entries.insert(entry.id.clone(), entry).is_some()
            {
                return Err("task record identity or privacy mismatch".into());
            }
        }
        profile::count("history_entries_read", history.entries.len() as u64);
        history.audit()?;
        Ok(history)
    }

    pub fn task(&self, id: &str) -> Result<&Task> {
        self.entries
            .values()
            .find_map(|e| match &e.event {
                Event::Begin { task } if e.task == id => Some(task.as_ref()),
                _ => None,
            })
            .ok_or_else(|| "unknown task".into())
    }

    pub fn rows(&self, task: &str) -> Vec<&Entry> {
        let mut rows: Vec<_> = self.entries.values().filter(|e| e.task == task).collect();
        rows.sort_by_key(|e| e.sequence);
        rows
    }

    pub fn outcome(&self, task: &str) -> Option<&Entry> {
        self.rows(task)
            .into_iter()
            .find(|e| matches!(e.event, Event::Finished { .. }))
    }

    pub fn usage(&self, task: &str) -> Usage {
        let mut out = Usage::default();
        for entry in self.rows(task) {
            match &entry.event {
                Event::ActionStarted { .. } => {
                    out.steps += 1;
                    out.pending_actions += 1;
                }
                Event::ActionCompleted { active_ms, .. } => {
                    out.active_ms = out.active_ms.saturating_add(*active_ms);
                    out.pending_actions = out.pending_actions.saturating_sub(1);
                }
                Event::Checked { active_ms, .. } => {
                    out.steps += 1;
                    out.active_ms = out.active_ms.saturating_add(*active_ms);
                }
                Event::Finished { active_ms, .. } => {
                    out.active_ms = out.active_ms.saturating_add(*active_ms)
                }
                Event::Overhead { active_ms, .. } => {
                    out.active_ms = out.active_ms.saturating_add(*active_ms)
                }
                _ => {}
            }
        }
        out
    }

    pub(crate) fn available(&self, task: &str) -> Result<u64> {
        let definition = &self.task(task)?.definition;
        let usage = self.usage(task);
        if self.outcome(task).is_some() {
            return Err("task is already finished; revise for a new attempt".into());
        }
        if usage.pending_actions != 0 {
            return Err("task has an unresolved execution; do not repeat it".into());
        }
        if usage.steps >= definition.budget.steps || usage.active_ms >= definition.budget.active_ms
        {
            return Err("task budget exhausted".into());
        }
        Ok(definition.budget.active_ms - usage.active_ms)
    }

    pub(crate) fn recent(&self, scope: &Scope) -> Vec<&Entry> {
        let mut rows: Vec<_> = self
            .entries
            .values()
            .filter(|e| {
                matches!(e.event, Event::Finished { .. })
                    && self.task(&e.task).is_ok_and(|t| &t.scope == scope)
            })
            .collect();
        rows.sort_by(|a, b| (b.at_ms, &b.id).cmp(&(a.at_ms, &a.id)));
        rows.truncate(32);
        rows
    }

    pub fn lessons(&self, scope: &Scope) -> Vec<Lesson> {
        let recent = self.recent(scope);
        let mut out = Vec::new();
        for kind in [
            LessonKind::InspectInputs,
            LessonKind::RetrieveSources,
            LessonKind::ProbeCost,
        ] {
            if kind == LessonKind::RetrieveSources && scope.family != "retrieval" {
                continue;
            }
            let (mut basis, mut contrary) = (Vec::new(), Vec::new());
            for entry in &recent {
                let Event::Finished { verdict, .. } = entry.event else {
                    continue;
                };
                let failure = if kind == LessonKind::ProbeCost {
                    self.task(&entry.task).is_ok_and(|t| {
                        self.usage(&entry.task).active_ms >= t.definition.budget.active_ms
                    })
                } else {
                    verdict == Verdict::Failure
                };
                if failure {
                    basis.push(entry.id.clone());
                } else if verdict == Verdict::Success {
                    contrary.push(entry.id.clone());
                }
            }
            if !basis.is_empty() {
                out.push(Lesson {
                    kind,
                    active: basis.len() >= 2 && basis.len() > contrary.len(),
                    basis,
                    contrary,
                    policy: POLICY.into(),
                });
            }
        }
        out
    }

    pub fn latest_checks(&self, task: &str) -> BTreeMap<String, &Entry> {
        let mut out = BTreeMap::new();
        for entry in self.rows(task) {
            if let Event::Checked { criterion, .. } = &entry.event {
                out.insert(criterion.clone(), entry);
            }
        }
        out
    }

    pub fn facts(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        for entry in self.entries.values() {
            let (id, t) = (constant(&entry.id), constant(&entry.task));
            writeln!(out, "exist({id}, TaskEvent, {t}).").unwrap();
            match &entry.event {
                Event::Begin { task } => {
                    writeln!(
                        out,
                        "sl_prediction({t}).\nsl_scope({t}, {}).",
                        constant(&hash(&task.scope))
                    )
                    .unwrap();
                    for criterion in &task.definition.criteria {
                        writeln!(
                            out,
                            "exist({}, TaskCriterion, {t}).",
                            constant(&criterion.id)
                        )
                        .unwrap();
                    }
                    let basis: BTreeSet<_> = task
                        .basis
                        .iter()
                        .chain(
                            task.lessons
                                .iter()
                                .flat_map(|l| l.basis.iter().chain(&l.contrary)),
                        )
                        .collect();
                    for evidence in basis {
                        let e = constant(evidence);
                        writeln!(out, "sl_basis({t}, {e}).").unwrap();
                        if self
                            .entries
                            .get(evidence)
                            .is_some_and(|e| e.at_ms <= entry.at_ms)
                        {
                            writeln!(out, "sl_precedes({e}, {t}).").unwrap();
                        }
                    }
                }
                Event::Checked { criterion, .. } => {
                    writeln!(out, "exist({id}, CriterionCheck, {}).", constant(criterion)).unwrap();
                }
                Event::Finished {
                    verdict, checks, ..
                } => {
                    writeln!(out, "sl_observation({id}, {t}, {verdict:?}).").unwrap();
                    for check in checks {
                        writeln!(out, "exist({id}, RequiredEvidence, {}).", constant(check))
                            .unwrap();
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn graph_invalid(&self) -> bool {
        let mut outcomes = BTreeSet::new();
        let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for entry in self.entries.values() {
            let Ok(task) = self.task(&entry.task) else {
                return true;
            };
            match &entry.event {
                Event::Begin { task } => {
                    for id in task.basis.iter().chain(
                        task.lessons
                            .iter()
                            .flat_map(|l| l.basis.iter().chain(&l.contrary)),
                    ) {
                        let Some(basis) = self.entries.get(id) else {
                            return true;
                        };
                        if !matches!(basis.event, Event::Finished { .. })
                            || basis.at_ms > entry.at_ms
                            || self
                                .task(&basis.task)
                                .map_or(true, |b| b.scope != task.scope)
                        {
                            return true;
                        }
                        edges.entry(&entry.task).or_default().push(&basis.task);
                    }
                }
                Event::Checked { criterion, .. } => {
                    if !task.definition.criteria.iter().any(|c| &c.id == criterion) {
                        return true;
                    }
                }
                Event::Finished { checks, .. }
                    if !outcomes.insert(&entry.task)
                        || checks.iter().any(|id| {
                            self.entries.get(id).is_none_or(|e| e.task != entry.task)
                        }) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        for start in edges.keys() {
            let mut pending = edges[start].clone();
            let mut seen = BTreeSet::new();
            while let Some(next) = pending.pop() {
                if &next == start {
                    return true;
                }
                if seen.insert(next) {
                    pending.extend(edges.get(next).into_iter().flatten());
                }
            }
        }
        false
    }

    pub fn audit(&self) -> Result<()> {
        let _span = profile::span("history_audit");
        if self.entries.is_empty() {
            return Ok(());
        }
        let nibli_span = profile::span("nibli_audit");
        let engine = NibliEngine::new();
        engine
            .assert_text(&format!("{SPEC}\n{}", self.facts()))
            .map_err(|e| e.to_string())?;
        let verdict = engine
            .query_holds("sl_invalid(History).")
            .map_err(|e| e.to_string())?;
        let (complete, _) = engine.materialization_report().map_err(|e| e.to_string())?;
        if !complete.iter().any(|s| s == "sl_invalid") {
            return Err("task evidence check incomplete".into());
        }
        drop(nibli_span);
        let _rust_span = profile::span("rust_audit");
        let invalid = match verdict {
            QueryResult::True => true,
            QueryResult::False => false,
            _ => return Err("task evidence check inconclusive".into()),
        };
        if invalid != self.graph_invalid() {
            return Err("Rust/Nibli task evidence checks disagree".into());
        }
        if invalid {
            return Err("invalid task evidence graph".into());
        }
        let ids: BTreeSet<_> = self.entries.values().map(|e| &e.task).collect();
        for id in ids {
            self.audit_task(id)?;
        }
        Ok(())
    }

    fn audit_task(&self, id: &str) -> Result<()> {
        let task = self.task(id)?;
        validate_definition(&task.definition)?;
        if task.checker != digest(SPEC.as_bytes())
            || task.forecaster != FORECASTER
            || !is_digest(&task.scope.workspace)
            || task.scope.source != task.definition.source
            || task.scope.family != task.definition.family
            || task.scope.method != task.definition.method
            || task.scope.model != task.definition.model
            || task.scope.environment != task.definition.environment
            || task.basis.len() > 32
            || task.basis.iter().collect::<BTreeSet<_>>().len() != task.basis.len()
            || task.probability_ppm != estimate(self, &task.basis)?
        {
            return Err("invalid task schema, version, scope or forecast".into());
        }
        let mut previous: Option<&Entry> = None;
        let mut pending = BTreeSet::new();
        let mut actions = BTreeSet::new();
        let mut complete = false;
        for (sequence, entry) in self.rows(id).into_iter().enumerate() {
            let mut content = entry.clone();
            content.id.clear();
            if entry.id != hash(&content)
                || entry.sequence != sequence as u64
                || entry.previous.as_deref() != previous.map(|e| e.id.as_str())
                || previous.is_some_and(|p| p.at_ms > entry.at_ms)
                || complete
            {
                return Err("task journal chain is invalid".into());
            }
            match &entry.event {
                Event::Begin { task } => {
                    if sequence != 0 || task.id != id {
                        return Err("invalid task beginning".into());
                    }
                    if let Some(old) = &task.revision_of
                        && (self.task(old)?.scope.source != task.scope.source
                            || self.outcome(old).is_none())
                    {
                        return Err("revision requires a closed task from the same source".into());
                    }
                    if task.lessons.len() > 3
                        || task
                            .lessons
                            .iter()
                            .map(|l| l.kind)
                            .collect::<BTreeSet<_>>()
                            .len()
                            != task.lessons.len()
                    {
                        return Err("duplicate or excessive lesson templates".into());
                    }
                    for lesson in &task.lessons {
                        if lesson.policy != POLICY
                            || lesson.active
                                != (lesson.basis.len() >= 2
                                    && lesson.basis.len() > lesson.contrary.len())
                            || lesson
                                .basis
                                .iter()
                                .chain(&lesson.contrary)
                                .collect::<BTreeSet<_>>()
                                .len()
                                != lesson.basis.len() + lesson.contrary.len()
                        {
                            return Err("invalid lesson support".into());
                        }
                        if lesson.kind == LessonKind::RetrieveSources
                            && task.scope.family != "retrieval"
                        {
                            return Err("retrieval lesson outside its task family".into());
                        }
                        for id in &lesson.basis {
                            let evidence = &self.entries[id];
                            let supported = if lesson.kind == LessonKind::ProbeCost {
                                self.usage(&evidence.task).active_ms
                                    >= self.task(&evidence.task)?.definition.budget.active_ms
                            } else {
                                matches!(
                                    evidence.event,
                                    Event::Finished {
                                        verdict: Verdict::Failure,
                                        ..
                                    }
                                )
                            };
                            if !supported {
                                return Err("lesson does not follow its failure evidence".into());
                            }
                        }
                        if lesson.contrary.iter().any(|id| {
                            !matches!(
                                self.entries[id].event,
                                Event::Finished {
                                    verdict: Verdict::Success,
                                    ..
                                }
                            )
                        }) {
                            return Err("lesson contrary evidence is not a success".into());
                        }
                    }
                }
                Event::ActionStarted {
                    action,
                    operation,
                    request_digest,
                } => {
                    if !pending.is_empty()
                        || !bounded(action)
                        || !bounded(operation)
                        || !is_digest(request_digest)
                        || !pending.insert(action)
                        || !actions.insert(action)
                    {
                        return Err("invalid or overlapping task execution".into());
                    }
                }
                Event::ActionCompleted { action, .. } => {
                    if !pending.remove(action) {
                        return Err("execution receipt has no preceding start".into());
                    }
                }
                Event::Overhead { stage, .. } => {
                    if !bounded(stage) || !pending.is_empty() {
                        return Err("invalid task overhead receipt".into());
                    }
                }
                Event::Checked {
                    verdict, evidence, ..
                } => {
                    if !pending.is_empty()
                        || !bounded(&evidence.reason)
                        || evidence
                            .files
                            .iter()
                            .chain(&evidence.records)
                            .any(|(p, h)| !bounded(p) || !is_digest(h))
                        || matches!(
                            evidence.class,
                            EvidenceClass::Reported | EvidenceClass::UserConfirmed
                        ) && *verdict != Verdict::Inconclusive
                    {
                        return Err("invalid task evidence receipt".into());
                    }
                }
                Event::Finished {
                    verdict,
                    checks,
                    reason,
                    ..
                } => {
                    let mut expected: Vec<_> = self
                        .latest_checks(id)
                        .values()
                        .map(|e| e.id.clone())
                        .collect();
                    expected.sort();
                    let mut actual = checks.clone();
                    actual.sort();
                    if expected != actual || !bounded(reason) {
                        return Err("task conclusion has incorrect evidence".into());
                    }
                    if *verdict != Verdict::Inconclusive
                        && (!pending.is_empty()
                            || self.usage(id).active_ms > task.definition.budget.active_ms
                            || *verdict != criteria_verdict(task, &self.latest_checks(id)))
                    {
                        return Err("unsupported task conclusion".into());
                    }
                    complete = true;
                }
            }
            previous = Some(entry);
        }
        if self.usage(id).steps > task.definition.budget.steps {
            return Err("task exceeded its step budget".into());
        }
        Ok(())
    }

    pub fn status(&self, task: Option<&str>) -> Result<Value> {
        self.audit()?;
        let ids: BTreeSet<_> = self
            .entries
            .values()
            .filter(|e| task.is_none_or(|id| id == e.task))
            .map(|e| &e.task)
            .collect();
        let tasks: Vec<_> = ids.into_iter().map(|id| {
            let definition = self.task(id).unwrap();
            let checks: Vec<_> = self.latest_checks(id).values().map(|e| &e.event).collect();
            json!({"task":definition,"usage":self.usage(id),"outcome":self.outcome(id).map(|e| &e.event),"checks":checks})
        }).collect();
        Ok(json!({"schema":1,"integrity":"checked","tasks":tasks,
            "meaning":"Criteria and evidence at capture time; imported reports are not native execution. No claim of consciousness."}))
    }
}

fn estimate(history: &History, basis: &[String]) -> Result<u32> {
    let mut successes = 0;
    for id in basis {
        match history.entries.get(id).map(|e| &e.event) {
            Some(Event::Finished {
                verdict: Verdict::Success,
                ..
            }) => successes += 1,
            Some(Event::Finished {
                verdict: Verdict::Failure,
                ..
            }) => {}
            _ => return Err("forecasts require conclusive prior task outcomes".into()),
        }
    }
    Ok((successes + 1) * 1_000_000 / (basis.len() as u32 + 2))
}

fn criteria_verdict(task: &Task, checks: &BTreeMap<String, &Entry>) -> Verdict {
    if checks.values().any(|e| {
        matches!(
            e.event,
            Event::Checked {
                verdict: Verdict::Failure,
                ..
            }
        )
    }) {
        return Verdict::Failure;
    }
    if checks.len() == task.definition.criteria.len()
        && checks.values().all(|e| {
            matches!(
                e.event,
                Event::Checked {
                    verdict: Verdict::Success,
                    ..
                }
            )
        })
    {
        Verdict::Success
    } else {
        Verdict::Inconclusive
    }
}

pub(crate) fn append(
    store: &Store,
    history: &mut History,
    task: &str,
    event: Event,
) -> Result<Entry> {
    let previous = history.rows(task).last().copied();
    let mut entry = Entry {
        id: String::new(),
        task: task.into(),
        sequence: previous.map_or(0, |e| e.sequence + 1),
        at_ms: now()?,
        previous: previous.map(|e| e.id.clone()),
        event,
    };
    entry.id = hash(&entry);
    history.entries.insert(entry.id.clone(), entry.clone());
    history.audit()?;
    let record = Record {
        origin: ORIGIN.into(),
        payload: json!({"id":entry.id,"private":true,
        "channel":"task_model","text":format!("Task evidence {}: receipt {}.",task,entry.sequence),"entry":entry}),
    };
    store.record(&record)?;
    if store.get(&record.origin, &entry.id)?.as_ref() != Some(&record) {
        return Err("task receipt readback failed".into());
    }
    Ok(entry)
}

pub fn begin(
    store: &Store,
    workspace: &Path,
    definition: Definition,
    observer: Observer,
) -> Result<Task> {
    begin_revision(store, workspace, definition, observer, None)
}
fn begin_revision(
    store: &Store,
    workspace: &Path,
    definition: Definition,
    observer: Observer,
    revision_of: Option<String>,
) -> Result<Task> {
    validate_definition(&definition)?;
    let workspace = checks::workspace_digest(workspace)?;
    let mut history = History::read(store)?;
    let scope = Scope {
        source: definition.source.clone(),
        observer,
        workspace,
        family: definition.family.clone(),
        method: definition.method.clone(),
        model: definition.model.clone(),
        environment: definition.environment.clone(),
    };
    let basis: Vec<_> = history
        .recent(&scope)
        .into_iter()
        .filter(|e| {
            !matches!(
                e.event,
                Event::Finished {
                    verdict: Verdict::Inconclusive,
                    ..
                }
            )
        })
        .map(|e| e.id.clone())
        .collect();
    let task = Task {
        id: identity("task")?,
        definition,
        scope: scope.clone(),
        checker: digest(SPEC.as_bytes()),
        forecaster: FORECASTER.into(),
        probability_ppm: estimate(&history, &basis)?,
        basis,
        lessons: history.lessons(&scope),
        revision_of,
    };
    append(
        store,
        &mut history,
        &task.id,
        Event::Begin {
            task: Box::new(task.clone()),
        },
    )?;
    Ok(task)
}

pub fn revise(
    store: &Store,
    workspace: &Path,
    old: &str,
    definition: Definition,
    observer: Observer,
) -> Result<Task> {
    let history = History::read(store)?;
    let original = history.task(old)?;
    if original.scope.observer != observer
        || original.definition.source != definition.source
        || original.definition.session != definition.session
    {
        return Err("revision belongs to another observer or session".into());
    }
    finish(store, workspace, old, true)?;
    begin_revision(store, workspace, definition, observer, Some(old.into()))
}

pub fn reserve_action(
    store: &Store,
    task: &str,
    operation: &str,
    request: &Value,
) -> Result<String> {
    let mut history = History::read(store)?;
    history.available(task)?;
    let action = identity("action")?;
    append(
        store,
        &mut history,
        task,
        Event::ActionStarted {
            action: action.clone(),
            operation: operation.into(),
            request_digest: hash(request),
        },
    )?;
    Ok(action)
}

pub fn complete_action(
    store: &Store,
    task: &str,
    action: &str,
    outcome: Verdict,
    active_ms: u64,
    exit_code: Option<i64>,
) -> Result<Entry> {
    let mut history = History::read(store)?;
    if let Some(old) = history
        .rows(task)
        .into_iter()
        .find(|e| matches!(&e.event,Event::ActionCompleted{action:a,..} if a==action))
    {
        if old.event
            == (Event::ActionCompleted {
                action: action.into(),
                outcome,
                active_ms,
                exit_code,
            })
        {
            return Ok(old.clone());
        }
        return Err("execution already has a different immutable receipt".into());
    }
    append(
        store,
        &mut history,
        task,
        Event::ActionCompleted {
            action: action.into(),
            outcome,
            active_ms,
            exit_code,
        },
    )
}

pub fn finish(store: &Store, workspace: &Path, task: &str, interrupted: bool) -> Result<Entry> {
    finish_with_clock(store, workspace, task, interrupted, None)
}

pub(crate) fn finish_with_clock(
    store: &Store,
    workspace: &Path,
    task: &str,
    interrupted: bool,
    clock: Option<&runtime::ActiveClock>,
) -> Result<Entry> {
    let _span = profile::span("task_finish");
    let started = Instant::now();
    let mut history = History::read(store)?;
    let definition = history.task(task)?.clone();
    checks::same_workspace(&definition, workspace)?;
    if let Some(old) = history.outcome(task) {
        return Ok(old.clone());
    }
    let checks = history.latest_checks(task);
    let remaining = definition
        .definition
        .budget
        .active_ms
        .saturating_sub(history.usage(task).active_ms)
        .min(clock.map_or(u64::MAX, |c| c.remaining_ms()));
    let fresh = checks.values().all(|e| match &e.event {
        Event::Checked {
            criterion,
            evidence,
            ..
        } => {
            let criterion = &definition
                .definition
                .criteria
                .iter()
                .find(|c| c.id == *criterion)
                .unwrap()
                .check;
            checks::fresh(
                store,
                workspace,
                criterion,
                evidence,
                remaining.saturating_sub(started.elapsed().as_millis() as u64),
            )
        }
        _ => false,
    });
    let ids = checks.values().map(|e| e.id.clone()).collect();
    let usage = history.usage(task);
    let elapsed = (started.elapsed().as_millis() as u64)
        .max(clock.map_or(0, |c| c.elapsed_ms().saturating_sub(usage.active_ms)));
    let (verdict, reason) = if interrupted || usage.pending_actions > 0 {
        (Verdict::Inconclusive, "interrupted_or_unresolved_execution")
    } else if usage.active_ms.saturating_add(elapsed) > definition.definition.budget.active_ms {
        (Verdict::Inconclusive, "active_budget_exhausted")
    } else if !fresh {
        (Verdict::Inconclusive, "evidence_stale_or_unavailable")
    } else {
        (
            criteria_verdict(&definition, &checks),
            "criteria_checked_with_missing_evidence_explicit",
        )
    };
    append(
        store,
        &mut history,
        task,
        Event::Finished {
            verdict,
            checks: ids,
            active_ms: elapsed,
            reason: reason.into(),
        },
    )
}

pub fn overhead(store: &Store, task: &str, stage: &str, active_ms: u64) -> Result<()> {
    let mut history = History::read(store)?;
    if history.outcome(task).is_none() {
        append(
            store,
            &mut history,
            task,
            Event::Overhead {
                stage: stage.into(),
                active_ms,
            },
        )?;
    }
    Ok(())
}
