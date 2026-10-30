use super::*;
use serde::Deserialize;
use std::path::PathBuf;

const DIMENSIONS: [&str; 5] = [
    "workspace",
    "method",
    "model_identity",
    "implementation",
    "environment",
];

fn reviewer_valid(r: &Reviewer) -> bool {
    nonempty(&r.source) && nonempty(&r.session)
}

fn support(store: &Store, evidence: &Evidence) -> Result<()> {
    let (actual, expected) = match evidence {
        Evidence::Record { origin, id, sha256 } => {
            let record = store
                .get(origin, id)?
                .ok_or("supporting record is missing")?;
            (hash(&record), sha256)
        }
        Evidence::File { path, sha256 } => {
            if !path.is_absolute() {
                return Err("supporting artifact paths must be absolute".into());
            }
            use std::io::Read;
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("support must be a regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("support exceeds eight MiB".into());
            }
            (digest(&bytes), sha256)
        }
    };
    if !is_digest(expected) || &actual != expected {
        return Err("support digest changed or is invalid".into());
    }
    Ok(())
}

impl History {
    fn validate_candidate(&self, target: &Group, c: &Candidate) -> Result<()> {
        let o = self
            .legacy
            .observations
            .get(&c.observation)
            .ok_or("candidate is not a legacy observation")?;
        let p = self
            .legacy
            .predictions
            .get(&o.prediction)
            .ok_or("legacy prediction is missing")?;
        if o.result == ResultKind::Inconclusive {
            return Err("inconclusive outcomes cannot train forecasts".into());
        }
        if hash(p) != c.prediction_digest || hash(o) != c.observation_digest {
            return Err("legacy record digest mismatch".into());
        }
        if target.source != p.scope.source
            || target.measurement != p.scope.measurement
            || target.capability != p.scope.capability
        {
            return Err("review cannot cross source, measurement or capability".into());
        }
        if target.measurement == Measurement::Runtime
            && target.conditions.workspace != p.scope.context
        {
            return Err("review conflicts with the recorded native workspace".into());
        }
        if matches!(
            target.conditions.model_identity,
            ModelIdentity::Unavailable { .. }
        ) {
            return Err("historical regrouping needs a supported model identity".into());
        }
        if c.rationale.len() != DIMENSIONS.len()
            || DIMENSIONS
                .iter()
                .any(|key| c.rationale.get(*key).is_none_or(|v| !nonempty(v)))
            || c.support.is_empty()
            || c.support.len() > 16
        {
            return Err("explain all five conditions and supply supporting references".into());
        }
        for s in &c.support {
            match s {
                Evidence::File { path, sha256 } if path.is_absolute() && is_digest(sha256) => {}
                Evidence::Record { origin, id, sha256 }
                    if nonempty(origin) && nonempty(id) && is_digest(sha256) => {}
                _ => return Err("invalid supporting reference".into()),
            }
        }
        Ok(())
    }
    pub(super) fn audit_reviews(&self) -> Result<()> {
        for r in self.reviews.values() {
            let mut original = r.clone();
            original.id.clear();
            let p = &r.preview;
            validate_group(&p.proposal.target)?;
            let mut unsigned = p.clone();
            unsigned.digest.clear();
            if r.id != format!("review-{}", hash(&original))
                || !reviewer_valid(&r.reviewer)
                || p.digest != hash(&unsigned)
                || !is_digest(&p.state_digest)
                || !p.excluded.is_empty()
                || p.included.is_empty()
                || p.proposal.candidates.len() > 256
                || p.included.windows(2).any(|w| w[0] >= w[1])
            {
                return Err("invalid immutable regrouping review".into());
            }
            let ids: BTreeSet<_> = p
                .proposal
                .candidates
                .iter()
                .map(|c| c.observation.clone())
                .collect();
            if ids.len() != p.proposal.candidates.len()
                || ids.into_iter().collect::<Vec<_>>() != p.included
            {
                return Err("review candidate list disagrees with included outcomes".into());
            }
            for c in &p.proposal.candidates {
                self.validate_candidate(&p.proposal.target, c)?;
                if self.legacy.observations[&c.observation].at_ms >= r.at_ms {
                    return Err("review predates its evidence".into());
                }
            }
        }
        for r in self.revocations.values() {
            let mut original = r.clone();
            original.id.clear();
            if r.id != format!("revocation-{}", hash(&original))
                || !reviewer_valid(&r.reviewer)
                || !nonempty(&r.reason)
                || self
                    .reviews
                    .get(&r.review)
                    .is_none_or(|review| review.at_ms >= r.at_ms)
            {
                return Err("invalid review revocation".into());
            }
        }
        Ok(())
    }
}

pub fn preview(store: &Store, mut proposal: Proposal) -> Result<Preview> {
    let h = History::read(store)?;
    h.audit()?;
    validate_group(&proposal.target)?;
    if proposal.candidates.is_empty() || proposal.candidates.len() > 256 {
        return Err("preview requires 1..256 explicit candidates".into());
    }
    proposal
        .candidates
        .sort_by(|a, b| a.observation.cmp(&b.observation));
    if proposal
        .candidates
        .windows(2)
        .any(|w| w[0].observation == w[1].observation)
    {
        return Err("duplicate review candidate".into());
    }
    let mut included = Vec::new();
    let mut excluded = BTreeMap::new();
    let mut candidates = h.candidates(&proposal.target, &h.controls(&proposal.target));
    for c in &proposal.candidates {
        let check = h.validate_candidate(&proposal.target, c).and_then(|()| {
            for s in &c.support {
                support(store, s)?;
            }
            Ok(())
        });
        match check {
            Ok(()) => {
                included.push(c.observation.clone());
                candidates.insert(c.observation.clone(), Some("preview".into()));
            }
            Err(error) => {
                excluded.insert(c.observation.clone(), error);
            }
        }
    }
    let basis = h.window(&candidates);
    let (probability_ppm, policy) = estimate(basis.iter().map(|id| h.observation(id).unwrap()));
    let mut p = Preview {
        proposal,
        state_digest: h.state_digest(),
        digest: String::new(),
        included,
        excluded,
        probability_ppm,
        policy,
        basis,
    };
    p.digest = hash(&p);
    Ok(p)
}

/// The digest is an explicit acknowledgement, not authentication of a human.
/// This is a CLI-only operation; it is deliberately absent from model tools.
pub fn apply(
    store: &Store,
    p: Preview,
    approved_preview_digest: &str,
    reviewer: Reviewer,
) -> Result<Review> {
    if approved_preview_digest != p.digest || !reviewer_valid(&reviewer) {
        return Err(
            "apply requires the approved preview digest and actual reviewer source/session".into(),
        );
    }
    let mut h = History::read(store)?;
    h.audit()?;
    if let Some(existing) = h
        .reviews
        .values()
        .find(|r| r.preview == p && r.reviewer == reviewer)
    {
        return Ok(existing.clone());
    }
    let fresh = preview(store, p.proposal.clone())?;
    if fresh != p {
        return Err("preview is stale or altered; inspect a fresh preview before approval".into());
    }
    if !p.excluded.is_empty() {
        return Err("unresolved comparability prevents apply".into());
    }
    let mut r = Review {
        id: String::new(),
        at_ms: h.next_time()?,
        preview: p,
        reviewer,
    };
    r.id = format!("review-{}", hash(&r));
    h.reviews.insert(r.id.clone(), r.clone());
    h.audit()?;
    save(store, Event::Review(r.clone()))?;
    Ok(r)
}

pub fn revoke(store: &Store, review: &str, reviewer: Reviewer, reason: &str) -> Result<Revocation> {
    let mut h = History::read(store)?;
    h.audit()?;
    if !reviewer_valid(&reviewer) || !nonempty(reason) || !h.reviews.contains_key(review) {
        return Err("revoke requires a known review, reviewer and reason".into());
    }
    if let Some(existing) = h.revocations.values().find(|r| r.review == review) {
        return if existing.reviewer == reviewer && existing.reason == reason {
            Ok(existing.clone())
        } else {
            Err("review already revoked".into())
        };
    }
    let mut r = Revocation {
        id: String::new(),
        at_ms: h.next_time()?,
        review: review.into(),
        reviewer,
        reason: reason.into(),
    };
    r.id = format!("revocation-{}", hash(&r));
    h.revocations.insert(r.id.clone(), r.clone());
    h.audit()?;
    save(store, Event::Revocation(r.clone()))?;
    Ok(r)
}

pub fn command(args: &[String], stdin: &str) -> Result<Value> {
    let operation = args.first().map(String::as_str).unwrap_or("");
    let mut root = None;
    let mut use_stdin = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--root" if root.is_none() => {
                i += 1;
                root = Some(PathBuf::from(
                    args.get(i).ok_or("--root requires a directory")?,
                ));
            }
            "--stdin" if !use_stdin => use_stdin = true,
            _ => return Err("regroup accepts preview|apply|revoke, --root and --stdin".into()),
        }
        i += 1;
    }
    if !use_stdin {
        return Err("regroup requires --stdin".into());
    }
    let root = root.map(Ok).unwrap_or_else(crate::store::default_root)?;
    if !root.is_dir() {
        return Err("regroup requires an existing private store".into());
    }
    let store = Store::open(&root)?;
    match operation {
        "preview" => Ok(json!(preview(
            &store,
            serde_json::from_str(stdin).map_err(|e| e.to_string())?
        )?)),
        "apply" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                preview: Preview,
                approved_preview_digest: String,
                reviewer: Reviewer,
            }
            let i: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            Ok(json!(apply(
                &store,
                i.preview,
                &i.approved_preview_digest,
                i.reviewer
            )?))
        }
        "revoke" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                review: String,
                reviewer: Reviewer,
                reason: String,
            }
            let i: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            Ok(json!(revoke(&store, &i.review, i.reviewer, &i.reason)?))
        }
        _ => Err("regroup requires preview, apply or revoke".into()),
    }
}
