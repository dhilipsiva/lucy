//! Versioned grouping. Legacy originals are read, never migrated or relabelled.
use super::{
    History as LegacyHistory, Measurement, Observation, ResultKind, constant, estimate, nonempty,
    now,
};
use crate::store::{Record, Store, digest};
use nibli_engine::NibliEngine;
use nibli_types::logic::QueryResult;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

mod reviews;
mod types;
pub use reviews::{apply, command as regroup_command, preview, revoke};
pub use types::*;
type Result<T> = std::result::Result<T, String>;
pub const ORIGIN: &str = "lucy:self-model:v2";
pub const SPEC: &str = include_str!("../../../constitution/self_model_v2.nibli");
const MODEL: &str = "empirical-window32-beta1-grouped-v2";

pub(super) fn hash(value: &impl Serialize) -> String {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .map(|(k, v)| (k, canonical(v)))
                    .collect(),
            ),
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            v => v,
        }
    }
    digest(
        &serde_json::to_vec(&canonical(
            serde_json::to_value(value).expect("serializable self-model value"),
        ))
        .unwrap(),
    )
}
pub fn session_digest(source: &str, session: &str) -> String {
    hash(&(source, session))
}
fn is_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(super) fn validate_group(group: &Group) -> Result<()> {
    let c = &group.conditions;
    if !nonempty(&group.source)
        || !nonempty(&group.capability)
        || !is_digest(&c.workspace)
        || !is_digest(&c.environment)
        || !nonempty(&c.method)
        || !nonempty(&c.implementation)
    {
        return Err("invalid grouped conditions".into());
    }
    match &c.model_identity {
        ModelIdentity::Known { name, revision }
            if nonempty(name) && revision.as_ref().is_none_or(|s| nonempty(s)) =>
        {
            Ok(())
        }
        ModelIdentity::Unavailable { session_digest } if is_digest(session_digest) => Ok(()),
        _ => Err("model identity must be named or explicitly session-scoped".into()),
    }
}

#[derive(Clone, Default)]
pub struct History {
    pub legacy: LegacyHistory,
    pub predictions: BTreeMap<String, Prediction>,
    pub observations: BTreeMap<String, Observation>,
    pub reviews: BTreeMap<String, Review>,
    pub revocations: BTreeMap<String, Revocation>,
}
impl History {
    pub fn read(store: &Store) -> Result<Self> {
        let mut h = Self {
            legacy: LegacyHistory::read(store)?,
            ..Self::default()
        };
        for r in store.all()?.into_iter().filter(|r| r.origin == ORIGIN) {
            let e: Event = serde_json::from_value(r.payload["event"].clone())
                .map_err(|e| format!("invalid v2 event: {e}"))?;
            if r.payload["id"] != e.id() {
                return Err("v2 record/event IDs disagree".into());
            }
            match e {
                Event::Prediction(p) => {
                    h.predictions.insert(p.id.clone(), p);
                }
                Event::Observation(o) => {
                    h.observations.insert(o.id.clone(), o);
                }
                Event::Review(r) => {
                    h.reviews.insert(r.id.clone(), r);
                }
                Event::Revocation(r) => {
                    h.revocations.insert(r.id.clone(), r);
                }
            }
        }
        Ok(h)
    }
    fn state_digest(&self) -> String {
        hash(&(
            &self.legacy.predictions,
            &self.legacy.observations,
            &self.predictions,
            &self.observations,
            &self.reviews,
            &self.revocations,
        ))
    }
    // Logical milliseconds are strictly increasing under Store's writer lock.
    fn next_time(&self) -> Result<u64> {
        let last = self
            .predictions
            .values()
            .map(|p| p.at_ms)
            .chain(self.observations.values().map(|o| o.at_ms))
            .chain(self.reviews.values().map(|r| r.at_ms))
            .chain(self.revocations.values().map(|r| r.at_ms))
            .chain(self.legacy.predictions.values().map(|p| p.at_ms))
            .chain(self.legacy.observations.values().map(|o| o.at_ms))
            .max()
            .unwrap_or(0);
        Ok(now()?.max(last.checked_add(1).ok_or("timestamp overflow")?))
    }
    fn controls(&self, group: &Group) -> Vec<String> {
        let mut controls: Vec<_> = self
            .reviews
            .values()
            .filter(|r| &r.preview.proposal.target == group)
            .map(|r| r.id.clone())
            .chain(
                self.revocations
                    .values()
                    .filter(|r| {
                        self.reviews
                            .get(&r.review)
                            .is_some_and(|review| &review.preview.proposal.target == group)
                    })
                    .map(|r| r.id.clone()),
            )
            .collect();
        controls.sort();
        controls
    }
    fn active_reviews<'a>(&'a self, state: &'a [String]) -> impl Iterator<Item = &'a Review> {
        let revoked: BTreeSet<_> = state
            .iter()
            .filter_map(|id| self.revocations.get(id))
            .map(|r| &r.review)
            .collect();
        state
            .iter()
            .filter_map(|id| self.reviews.get(id))
            .filter(move |r| !revoked.contains(&r.id))
    }
    fn observation(&self, id: &str) -> Option<&Observation> {
        self.observations
            .get(id)
            .or_else(|| self.legacy.observations.get(id))
    }
    fn candidates(&self, group: &Group, controls: &[String]) -> BTreeMap<String, Option<String>> {
        let mut rows: BTreeMap<_, _> = self
            .observations
            .values()
            .filter(|o| {
                o.result != ResultKind::Inconclusive
                    && self
                        .predictions
                        .get(&o.prediction)
                        .is_some_and(|p| &p.group() == group)
            })
            .map(|o| (o.id.clone(), None))
            .collect();
        for r in self.active_reviews(controls) {
            if &r.preview.proposal.target != group {
                continue;
            }
            for id in &r.preview.included {
                rows.entry(id.clone()).or_insert_with(|| Some(r.id.clone()));
            }
        }
        rows
    }
    fn window(&self, candidates: &BTreeMap<String, Option<String>>) -> Vec<String> {
        let mut rows: Vec<_> = candidates
            .keys()
            .filter_map(|id| self.observation(id))
            .collect();
        rows.sort_by(|a, b| (b.at_ms, &b.id).cmp(&(a.at_ms, &a.id)));
        rows.truncate(super::WINDOW);
        rows.into_iter().map(|o| o.id.clone()).collect()
    }
    fn graph_invalid(&self) -> bool {
        let mut resolved = BTreeSet::new();
        for o in self.observations.values() {
            let Some(p) = self.predictions.get(&o.prediction) else {
                return true;
            };
            if o.at_ms <= p.at_ms || !resolved.insert(&o.prediction) {
                return true;
            }
        }
        for p in self.predictions.values() {
            let group = p.group();
            let allowed = self.candidates(&group, &p.review_state);
            for id in &p.basis {
                let Some(o) = self.observation(id) else {
                    return true;
                };
                if o.at_ms >= p.at_ms || !allowed.contains_key(id) {
                    return true;
                }
                match p.review_basis.get(id) {
                    Some(review) => {
                        if !self.active_reviews(&p.review_state).any(|r| {
                            &r.id == review
                                && r.at_ms < p.at_ms
                                && r.preview.proposal.target == group
                                && r.preview.included.contains(id)
                        }) {
                            return true;
                        }
                    }
                    None if self
                        .predictions
                        .get(&o.prediction)
                        .is_none_or(|prior| prior.group() != group) =>
                    {
                        return true;
                    }
                    None => {}
                }
            }
            if p.review_basis.keys().any(|id| !p.basis.contains(id)) {
                return true;
            }
        }
        // Strictly earlier observations/predictions make a dependency cycle impossible.
        false
    }
    pub fn facts(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        for p in self.predictions.values() {
            let id = constant(&p.id);
            writeln!(
                out,
                "sl_prediction({id}).\nsl_scope({id}, {}).",
                constant(&p.group().key())
            )
            .unwrap();
            writeln!(
                out,
                "exist({id}, Boundary, {}).",
                constant(&hash(&(
                    &p.scope.measurement,
                    &p.scope.source,
                    &p.scope.capability
                )))
            )
            .unwrap();
            for basis in &p.basis {
                let b = constant(basis);
                writeln!(out, "sl_basis({id}, {b}).").unwrap();
                if self.observation(basis).is_some_and(|o| o.at_ms < p.at_ms) {
                    writeln!(out, "sl_precedes({b}, {id}).").unwrap();
                }
            }
            for (o, r) in &p.review_basis {
                let use_id = constant(&hash(&(&p.id, o, r)));
                writeln!(out,"exist({use_id}, ReviewUse, {id}).\nexist({use_id}, UsesObservation, {}).\nexist({use_id}, UsesReview, {}).",constant(o),constant(r)).unwrap();
            }
            for r in self
                .active_reviews(&p.review_state)
                .filter(|r| r.at_ms < p.at_ms)
            {
                writeln!(out, "exist({}, ActiveFor, {id}).", constant(&r.id)).unwrap();
            }
        }
        for p in self.legacy.predictions.values() {
            writeln!(
                out,
                "sl_prediction({}).\nsl_scope({}, {}).",
                constant(&p.id),
                constant(&p.id),
                constant(&format!("legacy:{}", super::scope_key(&p.scope)))
            )
            .unwrap();
            writeln!(
                out,
                "exist({}, Boundary, {}).",
                constant(&p.id),
                constant(&hash(&(
                    &p.scope.measurement,
                    &p.scope.source,
                    &p.scope.capability
                )))
            )
            .unwrap();
        }
        for o in self
            .observations
            .values()
            .chain(self.legacy.observations.values())
        {
            writeln!(
                out,
                "sl_observation({}, {}, Outcome).",
                constant(&o.id),
                constant(&o.prediction)
            )
            .unwrap();
            let earlier = self
                .predictions
                .get(&o.prediction)
                .map(|p| p.at_ms < o.at_ms)
                .or_else(|| {
                    self.legacy
                        .predictions
                        .get(&o.prediction)
                        .map(|p| p.at_ms <= o.at_ms)
                })
                .unwrap_or(false);
            if earlier {
                writeln!(
                    out,
                    "sl_precedes({}, {}).",
                    constant(&o.prediction),
                    constant(&o.id)
                )
                .unwrap();
            }
            if o.result != ResultKind::Inconclusive {
                writeln!(out, "exist({}, Conclusive, Observation).", constant(&o.id)).unwrap();
            }
        }
        for r in self.reviews.values() {
            writeln!(out, "exist({}, Review, History).", constant(&r.id)).unwrap();
            let g = &r.preview.proposal.target;
            writeln!(
                out,
                "exist({}, Boundary, {}).",
                constant(&r.id),
                constant(&hash(&(&g.measurement, &g.source, &g.capability)))
            )
            .unwrap();
            for id in &r.preview.included {
                let member = constant(&hash(&(&r.id, id)));
                writeln!(out,"exist({member}, ReviewMember, {}).\nexist({member}, MemberObservation, {}).\nexist({member}, MemberGroup, {}).",constant(&r.id),constant(id),constant(&r.preview.proposal.target.key())).unwrap();
                if self
                    .legacy
                    .observations
                    .get(id)
                    .is_some_and(|o| o.at_ms < r.at_ms)
                {
                    writeln!(out, "sl_precedes({}, {}).", constant(id), constant(&r.id)).unwrap();
                }
            }
        }
        out
    }
    pub fn audit(&self) -> Result<()> {
        self.legacy.audit()?;
        self.audit_reviews()?;
        let mut ids = BTreeSet::new();
        for id in self
            .predictions
            .keys()
            .chain(self.observations.keys())
            .chain(self.reviews.keys())
            .chain(self.revocations.keys())
        {
            if !ids.insert(id)
                || self.legacy.predictions.contains_key(id)
                || self.legacy.observations.contains_key(id)
            {
                return Err("duplicate v2 identity".into());
            }
        }
        for p in self.predictions.values() {
            validate_group(&p.group())?;
            if p.scope != p.group().scope()
                || p.model != MODEL
                || p.checker != digest(SPEC.as_bytes())
                || !nonempty(&p.id)
                || !nonempty(&p.session)
                || !nonempty(&p.expectation)
                || p.basis.len() > super::WINDOW
                || p.basis.iter().collect::<BTreeSet<_>>().len() != p.basis.len()
                || p.metadata.len() > 32
                || serde_json::to_vec(&p.metadata).unwrap().len() > 8192
                || p.metadata.iter().any(|(k, v)| !nonempty(k) || !nonempty(v))
            {
                return Err("invalid v2 prediction schema or version".into());
            }
            if let ModelIdentity::Unavailable { session_digest } = &p.conditions.model_identity
                && session_digest != &hash(&(&p.scope.source, &p.session))
            {
                return Err("unknown model must remain session-scoped".into());
            }
            if p.review_state.windows(2).any(|w| w[0] >= w[1]) {
                return Err("review snapshot must be sorted and unique".into());
            }
            for id in &p.review_state {
                let (review, at) = if let Some(r) = self.reviews.get(id) {
                    (r, r.at_ms)
                } else if let Some(r) = self.revocations.get(id) {
                    (&self.reviews[&r.review], r.at_ms)
                } else {
                    return Err("missing review snapshot record".into());
                };
                if at >= p.at_ms
                    || review.preview.proposal.target != p.group()
                    || !p.review_state.contains(&review.id)
                {
                    return Err("invalid review snapshot".into());
                }
            }
        }
        for o in self.observations.values() {
            if o.id != format!("outcome-{}", o.prediction) || !nonempty(&o.evidence) {
                return Err("invalid v2 observation".into());
            }
        }
        let engine = NibliEngine::new();
        engine
            .assert_text(&format!("{SPEC}\n{}", self.facts()))
            .map_err(|e| e.to_string())?;
        let verdict = engine
            .query_holds("sl_invalid(History).")
            .map_err(|e| e.to_string())?;
        let (complete, _) = engine.materialization_report().map_err(|e| e.to_string())?;
        if !complete.iter().any(|s| s == "sl_invalid") {
            return Err("v2 evidence check incomplete".into());
        }
        let invalid = match verdict {
            QueryResult::True => true,
            QueryResult::False => false,
            _ => return Err("v2 evidence check inconclusive".into()),
        };
        if invalid != self.graph_invalid() {
            return Err("Rust/Nibli v2 evidence checks disagree".into());
        }
        if invalid {
            return Err("invalid v2 evidence graph".into());
        }
        for p in self.predictions.values() {
            if estimate(p.basis.iter().map(|id| self.observation(id).unwrap()))
                != (p.probability_ppm, p.policy)
            {
                return Err("v2 estimate does not follow its evidence".into());
            }
        }
        Ok(())
    }
    pub fn summary(&self) -> Result<Value> {
        self.audit()?;
        let groups: BTreeSet<_> = self
            .predictions
            .values()
            .map(Prediction::group)
            .chain(
                self.reviews
                    .values()
                    .map(|r| r.preview.proposal.target.clone()),
            )
            .collect();
        let mut rows = Vec::new();
        for group in groups {
            let candidates = self.candidates(&group, &self.controls(&group));
            let basis = self.window(&candidates);
            let (probability, policy) =
                estimate(basis.iter().map(|id| self.observation(id).unwrap()));
            let scores: Vec<f64> = self
                .observations
                .values()
                .filter(|o| o.result != ResultKind::Inconclusive)
                .filter_map(|o| {
                    self.predictions
                        .get(&o.prediction)
                        .filter(|p| p.group() == group)
                        .map(|p| {
                            (p.probability_ppm as f64 / 1_000_000.
                                - if o.result == ResultKind::Success {
                                    1.
                                } else {
                                    0.
                                })
                            .powi(2)
                        })
                })
                .collect();
            let brier =
                (!scores.is_empty()).then(|| scores.iter().sum::<f64>() / scores.len() as f64);
            rows.push(json!({"group":group,"group_digest":group.key(),"grouping_version":2,
                "observations":basis.len(),"reviewed_observations":basis.iter().filter(|id| candidates[*id].is_some()).count(),
                "probability_ppm":probability,"policy":policy,"basis":basis,"brier_score":brier,"scored_forecasts":scores.len()}));
        }
        Ok(
            json!({"grouping_version":2,"model":MODEL,"checker":digest(SPEC.as_bytes()),"integrity":"checked",
            "predictions":self.predictions.len(),"observations":self.observations.len(),"pending":self.predictions.len()-self.observations.len(),
            "reviews":self.reviews.len(),"revocations":self.revocations.len(),"scopes":rows}),
        )
    }
}

fn save(store: &Store, event: Event) -> Result<()> {
    let record = Record {
        origin: ORIGIN.into(),
        payload: json!({"id":event.id(),"private":true,
        "channel":"self_model","text":format!("Grouped self-model evidence {}.",event.id()),"event":event}),
    };
    store.record(&record)?;
    if store
        .get(ORIGIN, record.payload["id"].as_str().unwrap())?
        .as_ref()
        != Some(&record)
    {
        return Err("v2 record readback failed".into());
    }
    Ok(())
}

pub fn predict(
    store: &Store,
    group: Group,
    session: &str,
    expectation: &str,
    metadata: BTreeMap<String, String>,
) -> Result<Prediction> {
    let mut h = History::read(store)?;
    h.audit()?;
    validate_group(&group)?;
    let review_state = h.controls(&group);
    let candidates = h.candidates(&group, &review_state);
    let basis = h.window(&candidates);
    let review_basis = basis
        .iter()
        .filter_map(|id| candidates[id].as_ref().map(|r| (id.clone(), r.clone())))
        .collect();
    let (probability_ppm, policy) = estimate(basis.iter().map(|id| h.observation(id).unwrap()));
    let mut p = Prediction {
        forecast: super::Prediction {
            id: String::new(),
            at_ms: h.next_time()?,
            scope: group.scope(),
            session: session.into(),
            expectation: expectation.into(),
            model: MODEL.into(),
            checker: digest(SPEC.as_bytes()),
            basis,
            probability_ppm,
            policy,
        },
        conditions: group.conditions,
        metadata,
        review_state,
        review_basis,
    };
    p.forecast.id = format!("prediction-v2-{}", hash(&p));
    h.predictions.insert(p.id.clone(), p.clone());
    h.audit()?;
    save(store, Event::Prediction(p.clone()))?;
    Ok(p)
}

pub fn observe(
    store: &Store,
    prediction: &str,
    result: ResultKind,
    evidence: &str,
) -> Result<Observation> {
    let mut h = History::read(store)?;
    h.audit()?;
    if !h.predictions.contains_key(prediction) {
        return Err("unknown v2 prediction".into());
    }
    let id = format!("outcome-{prediction}");
    if let Some(old) = h.observations.get(&id) {
        return if old.result == result && old.evidence == evidence {
            Ok(old.clone())
        } else {
            Err("prediction already has a different immutable outcome".into())
        };
    }
    let o = Observation {
        id,
        prediction: prediction.into(),
        at_ms: h.next_time()?,
        result,
        evidence: evidence.into(),
    };
    h.observations.insert(o.id.clone(), o.clone());
    h.audit()?;
    save(store, Event::Observation(o.clone()))?;
    Ok(o)
}

pub fn prediction_json(p: &Prediction) -> Value {
    let mut value = serde_json::to_value(&p.forecast).unwrap();
    value["grouping_version"] = json!(2);
    value["conditions"] = json!(p.conditions);
    value["metadata"] = json!(p.metadata);
    value["review_state"] = json!(p.review_state);
    value["review_basis"] = json!(p.review_basis);
    value["evidence_count"] = json!(p.basis.len());
    value["reviewed_evidence_count"] = json!(p.review_basis.len());
    value
}

#[cfg(test)]
mod tests;
