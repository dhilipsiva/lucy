use super::super::{Measurement, Observation, Prediction as LegacyPrediction, Scope};
use crate::store::digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelIdentity {
    Known {
        name: String,
        revision: Option<String>,
    },
    Unavailable {
        session_digest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub workspace: String,
    pub method: String,
    pub model_identity: ModelIdentity,
    pub implementation: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputConditions {
    pub workspace: PathBuf,
    pub method: String,
    pub model_identity: ModelIdentity,
    pub implementation: String,
    pub environment: String,
}
impl InputConditions {
    pub fn resolve(self) -> Result<Conditions, String> {
        let workspace = self.workspace.canonicalize().map_err(|e| e.to_string())?;
        if !workspace.is_dir() || !super::super::nonempty(&self.environment) {
            return Err(
                "conditions require a directory and bounded environment description".into(),
            );
        }
        Ok(Conditions {
            workspace: digest(workspace.as_os_str().as_encoded_bytes()),
            method: self.method,
            model_identity: self.model_identity,
            implementation: self.implementation,
            environment: digest(self.environment.as_bytes()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub measurement: Measurement,
    pub source: String,
    pub capability: String,
    pub conditions: Conditions,
}
impl Group {
    pub fn scope(&self) -> Scope {
        Scope {
            measurement: self.measurement,
            source: self.source.clone(),
            capability: self.capability.clone(),
            context: super::hash(&(2, &self.conditions)),
        }
    }
    pub fn key(&self) -> String {
        super::hash(&(2, self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prediction {
    pub forecast: LegacyPrediction,
    pub conditions: Conditions,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// The exact control records visible at prediction time, including revocations.
    pub review_state: Vec<String>,
    /// Observation -> review; direct v2 evidence has no entry here.
    pub review_basis: BTreeMap<String, String>,
}
impl std::ops::Deref for Prediction {
    type Target = LegacyPrediction;
    fn deref(&self) -> &Self::Target {
        &self.forecast
    }
}
impl Prediction {
    pub fn group(&self) -> Group {
        Group {
            measurement: self.scope.measurement,
            source: self.scope.source.clone(),
            capability: self.scope.capability.clone(),
            conditions: self.conditions.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Evidence {
    Record {
        origin: String,
        id: String,
        sha256: String,
    },
    File {
        path: PathBuf,
        sha256: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub observation: String,
    pub prediction_digest: String,
    pub observation_digest: String,
    /// A separate explanation for each of the five conditions, supported below.
    pub rationale: BTreeMap<String, String>,
    pub support: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub target: Group,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub proposal: Proposal,
    pub state_digest: String,
    pub digest: String,
    pub included: Vec<String>,
    pub excluded: BTreeMap<String, String>,
    pub probability_ppm: u32,
    pub policy: super::super::Policy,
    pub basis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reviewer {
    pub source: String,
    pub session: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub id: String,
    pub at_ms: u64,
    pub preview: Preview,
    pub reviewer: Reviewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revocation {
    pub id: String,
    pub at_ms: u64,
    pub review: String,
    pub reviewer: Reviewer,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Event {
    Prediction(Prediction),
    Observation(Observation),
    Review(Review),
    Revocation(Revocation),
}
impl Event {
    pub fn id(&self) -> &str {
        match self {
            Self::Prediction(p) => &p.id,
            Self::Observation(o) => &o.id,
            Self::Review(r) => &r.id,
            Self::Revocation(r) => &r.id,
        }
    }
}
