# Comparable execution forecasts

Grouping version 2 separates comparable conditions from incidental run details.
It covers reported CLI forecasts and native execution. The estimator remains
`(successes + 1) / (successes + failures + 2)` over the latest 32 conclusive
outcomes. Two failures that outnumber successes still select `inspect_first`.
Execution success remains separate from task acceptance.

## Conditions and run details

The group includes measurement kind, actual source, capability, and five conditions:

| Condition | Meaning |
| --- | --- |
| `workspace` | Canonical directory digest; the CLI accepts an existing directory. |
| `method` | Stable identifier for the procedure being predicted. |
| `model_identity` | Named model and optional revision, or an explicitly unavailable identity scoped to one source/session. |
| `implementation` | Implementation identity for that procedure; the native executor uses its binary digest. |
| `environment` | Relevant environment description; the CLI hashes it before storage. |

Keys are deterministic. A timestamp, session ID, expectation or metadata change
does not change a named-model group. A change to any condition does. A configured
model name with `revision: null` is explicitly weaker evidence than known weights;
it does not prove a hosted service kept the same weights. An entirely unavailable
model identity cannot share evidence across sessions. The CLI must receive the
actual external model identity, not the local service's selected provider.

Native inference captures the provider actually used and carries its identity
with the tool proposal through approval. Direct external tool requests have an
unavailable identity. Native environment fingerprints include OS/architecture,
the executor contract, and applicable command-path or helper configuration.
Paths/configuration are hashed; no new inference is invoked for grouping.
The existing tool-operation categories are unchanged; a command execution
forecast is not a claim that arbitrary commands solve the same task.

Example input for `lucy self-model predict --stdin`:

```json
{
  "source": "codex",
  "session": "ACTUAL_SESSION_ID",
  "capability": "peer-sync",
  "expectation": "The specified peer synchronization completes.",
  "conditions": {
    "workspace": "/path/to/existing/workspace",
    "method": "all-enrolled-peers-v1",
    "model_identity": {"kind": "known", "name": "ACTUAL_MODEL_ID", "revision": null},
    "implementation": "VERIFIED_SYNC_IMPLEMENTATION",
    "environment": "IDENTIFIED_RELEVANT_ENVIRONMENT"
  },
  "metadata": {"date": "2026-10-30", "system_generation": "29"}
}
```

Declare which conditions matter before the attempt; do not hide a meaningful
environment change in metadata. The mechanism does not infer comparability from
natural language. The old `context` string remains supported as v1; combining it
with `conditions` is rejected. Metadata requires v2, is limited to 32 bounded
string pairs / 8 KiB, and is absent from wake summaries. Use the existing
`observe` command after execution. CLI reports cannot resolve native predictions.

`status` adds a `grouped` section. `events` retains the old maps and adds grouped
predictions, observations, reviews and revocations. New prediction responses
include the grouping version and direct/reviewed evidence counts. Group summaries
score only actual conclusive v2 forecasts in that group; imported outcomes are
counted separately and never acquire a fabricated forecast. The legacy Brier
scores and forecast records do not change.

## Reviewing legacy outcomes

Reviews are explicit human interpretations supported by readable sources. Content
digests establish which evidence was reviewed, not whether a rationale is true.
Nothing automatically merges old contexts or adds a review tool to model schemas.

1. Inspect `self-model events`. Its `legacy_digests.predictions` and
   `legacy_digests.observations` provide the exact digests needed below. Choose
   explicit conclusive observation IDs and a target group from a structured
   forecast or the grouped status view.
2. Supply a proposal to `self-model regroup preview --stdin`. A proposal contains
   `target` (measurement, source, capability, stored conditions) and `candidates`.
   Each candidate has `observation`, `prediction_digest`, `observation_digest`,
   `rationale`, and `support`. Rationale must separately explain `workspace`,
   `method`, `model_identity`, `implementation`, and `environment`.
3. Supporting references are either
   `{"kind":"record","origin":"...","id":"...","sha256":"..."}` or
   `{"kind":"file","path":"/absolute/artifact","sha256":"..."}`.
   Record digests use the complete Store record, not just its text. Structured
   digests are SHA-256 of compact UTF-8 JSON with recursively sorted object keys;
   file digests use raw bytes. Files must be regular and at most eight MiB.
4. Inspect the returned included/excluded lists, conditions, rationales, sources,
   estimated probability and policy. Preview records no events. Missing evidence,
   mismatched hashes or incomplete explanations block application. Source,
   measurement and capability cannot change; a native legacy workspace cannot
   change either. Unavailable model identity cannot establish historical reuse.
5. After the user approves that exact preview, call
   `self-model regroup apply --stdin` with
   `{"preview":PREVIEW_OBJECT,"approved_preview_digest":"PREVIEW_DIGEST","reviewer":{"source":"ACTUAL_SOURCE","session":"ACTUAL_SESSION_ID"}}`.
   A changed self-model archive or supporting artifact requires a fresh preview and approval.
   The acknowledgement is not cryptographic authentication of a human. The agent
   must not supply it on the user's behalf without that approval.
6. To withdraw a review, call `self-model regroup revoke --stdin` with
   `{"review":"REVIEW_ID","reviewer":{"source":"ACTUAL_SOURCE","session":"ACTUAL_SESSION_ID"},"reason":"Correction and its basis."}`.

All commands accept `--root DIR`. Preview/apply accept 1–256 explicit candidates;
each candidate needs 1–16 references. Duplicate candidates are rejected. Multiple
reviews of the same outcome count it once; original timestamps determine the
32-outcome window. Identical apply/revoke retries are idempotent; retrying an old
apply does not reactivate a revoked review.

A forecast captures the review/revocation records visible at creation and the
review used for each imported outcome. Revocation affects future forecasts only.
Later arrival of replicated control records does not rewrite an earlier snapshot.
Evidence is read back at preview and apply; audit preserves that historical
review even if its supporting artifact later disappears. Reproducing the human
review requires those originals. A changed interpretation should be revoked.

## Compatibility and evidence boundaries

New originals live under `lucy:self-model:v2` in the existing private store. The
unchanged v1 reader ignores that origin; new readers audit both versions. Old
context-string calls and observations retain their original format. Backup,
restore and reindex preserve both origins. There is no automatic live migration
or new publication route. The running packaged service requires its normal
deployment process before it uses new native behavior.

Rust validates schema, versions, hashes, arithmetic, review snapshots and review
source bindings. A separate versioned Nibli specification uses the existing
pinned vocabulary to check the evidence graph. Agreement establishes consistency
of the supplied records. The reifier, clock, storage, model-identity resolver,
execution classification, and human comparability judgment remain trust boundaries.
Unknown, incomplete or conflicting checks reject an update.

The preregistration is [S6a-grouping](../roadmap/S6a-grouping.md). Mechanism tests
do not establish improved task performance. The independent S6a benefit study,
model-change experiment and policy activation gate remain separate.
