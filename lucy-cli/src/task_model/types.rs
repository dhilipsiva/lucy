use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observer {
    Native,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub source: String,
    pub observer: Observer,
    /// Canonical workspace digest, not a private pathname.
    pub workspace: String,
    pub family: String,
    pub method: String,
    pub model: String,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    FileExists {
        path: String,
    },
    FileSha256 {
        path: String,
        sha256: String,
    },
    JsonEquals {
        path: String,
        pointer: String,
        expected: Value,
    },
    Record {
        origin: String,
        record: String,
        sha256: String,
    },
    NibliReport {
        path: String,
    },
    LeanReport {
        path: String,
    },
    /// An explicit unsupported/manual requirement cannot silently disappear.
    Reported {
        statement: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub check: Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub steps: u32,
    pub active_ms: u64,
}
impl Default for Budget {
    fn default() -> Self {
        // Twelve existing command slots, each bounded by the executor's 120s cap.
        Self {
            steps: 12,
            active_ms: 12 * 120_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub source: String,
    pub session: String,
    pub title: String,
    pub family: String,
    pub method: String,
    pub model: String,
    pub environment: String,
    pub criteria: Vec<Criterion>,
    #[serde(default)]
    pub budget: Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Success,
    Failure,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    Observed,
    Inspected,
    Reported,
    UserConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonKind {
    InspectInputs,
    RetrieveSources,
    ProbeCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub kind: LessonKind,
    pub active: bool,
    pub basis: Vec<String>,
    pub contrary: Vec<String>,
    pub policy: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub definition: Definition,
    pub scope: Scope,
    pub checker: String,
    pub forecaster: String,
    pub basis: Vec<String>,
    pub probability_ppm: u32,
    pub lessons: Vec<Lesson>,
    pub revision_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub class: EvidenceClass,
    /// Digest-bound files and records actually inspected, with bounded locators.
    pub files: BTreeMap<String, String>,
    pub records: BTreeMap<String, String>,
    pub missing_files: Vec<String>,
    pub missing_records: Vec<String>,
    pub reason: String,
    /// Source evidence may be locally available without being archived in full.
    pub replay_available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Begin {
        task: Box<Task>,
    },
    ActionStarted {
        action: String,
        operation: String,
        request_digest: String,
    },
    ActionCompleted {
        action: String,
        outcome: Verdict,
        active_ms: u64,
        exit_code: Option<i64>,
    },
    Overhead {
        stage: String,
        active_ms: u64,
    },
    Checked {
        criterion: String,
        verdict: Verdict,
        evidence: Evidence,
        active_ms: u64,
    },
    Finished {
        verdict: Verdict,
        checks: Vec<String>,
        active_ms: u64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub task: String,
    pub sequence: u64,
    pub at_ms: u64,
    pub previous: Option<String>,
    pub event: Event,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub steps: u32,
    pub active_ms: u64,
    pub pending_actions: usize,
}
