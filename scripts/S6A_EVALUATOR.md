# S6a v2 independent evaluator contract

This is a frozen interface and protocol guide, not experimental case material.
Follow the mode named in your invocation: selection or adjudication. Stay inside
the supplied private study directory. Do not load Lucy, use memory integrations,
read other conversations, contact peers, publish, or run either evaluated model.
Use only synthetic information. Your configured host model and permissions stay
unchanged; a fresh session provides separation from the builder, not proof of
independent training or a second provider. Preserve your actual adapter identity.

## Selection

Read S6a.v2.md and freeze.json. Prepare one sealed bundle shared by both evaluated
models, with separate calibration histories supplied by the runner. Make tasks
meaningful and varied within each family, with independently defensible ground
truth. They should test whether input/source inspection learned through failure
helps on unfamiliar tasks. Do not prescribe failures, force outcomes, plant
instructions to activate lessons, or design guaranteed wins. Do not imitate the
builder's unit fixtures. Do not tune after observing any model execution.

Create 10 calibration and 20 different held-out cases in each of verification
and retrieval. Fix one nonempty method string per family across both sets. Each
case has presentations a and b describing the same actual task in different
words; ground truth, facts and budgets are identical across presentations and
arms. At most twelve action/check steps and 1,440,000 active milliseconds per
task are available; pick practical smaller budgets if appropriate, fixed before
sealing and identical for both models. Global time includes preparation and is
one hour per model. There are 340 episodes per model; do not run any of them.

The actual native tools available to the evaluated model are:

- `list({path?})`, `read({path})`, `search({query})`: workspace-only reads.
- `write({path,text})`: atomic whole-file replacement, approved only for a
  declared output path and byte bound, regardless of whether the answer is right.
- `command({argv})`: requires exact preapproval and the existing offline sandbox.
  Prefer the native read/write tools; do not assume any command or network access.
- `task_status({})`, `task_check({criterion})`, `task_finish({})`: inspect the
  already frozen contract, check one criterion, then conclude. The model must
  inspect all required criteria for verified success. `task_begin/task_revise`
  cannot replace this task's contract during its turn.

Maximum twelve model replies and one tool per reply. The ordinary model context
limit applies. Keep presentations, descriptions and contracts concise; their
combined text plus the 6–8 KB research roadmap must fit 18,000 UTF-8 bytes.
Explain the user-facing artifact format, calculations and source-selection rules
well enough to solve the task from its factual inputs. Do not put the semantic
answer in titles, descriptions, presentations, write approvals or initial files.

The task checker retains semantic expectations privately. Initial contracts and
task-status replies expose structural fields, without `expected` or `sha256`.
`task_check` provides a bounded pass/fail observation, never the expected value.
Define meaningful native acceptance criteria, not merely output existence.
The separate oracle must independently inspect produced output correctness.
For retrieval it must also check a JSON `/source` value identifying the correct
source. A source's availability alone never establishes the answer was produced.

Factual memory uses declared private `fixture:` records. Normal recall ranks
only the current case's declared records lexically against its prompt, takes up
to six matches, and exposes origin, id and the first 1,600 Unicode characters.
No journals or old conversations enter recall. There is no general memory tool.
The memory-removed arm starts with no source records or calibration history.

The registered task learning mechanism uses conclusive outcomes in the exact
family/method/model/workspace scope. Two failures exceeding successes can
activate bounded templates. Verification templates inspect required input file
metadata (availability, size and parent existence). Retrieval templates retrieve
the exact source records named by `record` criteria, taking up to 2,000 characters
from current-case allowlisted records. These inspections consume task steps and
time; they never execute a command or increase a budget. Old execution-model
adaptation is suppressed in every study arm. If calibration activates nothing,
that is a valid null outcome. Do not manufacture a lesson in the bundle.

Bundle JSON shape (all fields below are required unless explicitly optional):

```text
schema: 2
evaluator: actual CODEX_THREAD_ID supplied by this fresh adapter
evaluator_model: evaluator.model from freeze.json
implementation, protocol: exactly those digests from freeze.json
calibration: array of 20 cases (10 per family)
held_out: array of 40 cases (20 per family)

case:
  id: unique nonempty string across both arrays
  disguises: {a: user task text, b: independently worded task text}
  definition:
    source, session, model, environment: nonempty placeholders replaced by runtime
    title: concise task description without an answer
    family: verification or retrieval
    method: one fixed string per family
    criteria: 1..12 Criterion objects
    budget: {steps: integer 1..12, active_ms: integer 1..1440000}
  oracle: 1..12 Criterion objects with defensible ground truth
  output_writes: array of {path: ordinary relative path, max_bytes: 1..1048576}
  files: optional map of ordinary relative input paths to UTF-8 contents
  records: optional array of Record objects
  approvals: optional array of {tool: "command", arguments: {argv: [...]}}

Criterion: {id: unique string, description: nonempty text, check: {...}}
check variants, with no additional keys:
  {kind:"file_exists", path:relative_path}
  {kind:"file_sha256", path:relative_path, sha256:64_hex_digest}
  {kind:"json_equals", path:relative_path, pointer:JSON_pointer, expected:JSON_value}
  {kind:"record", origin:source_origin, record:source_id, sha256:64_hex_digest}
Record: {origin:"fixture:...", payload:{id:source_id, text:source_text,
                                     private:true, channel:"fixture"}}
```

Record digests are SHA-256 of compact UTF-8 JSON with object keys sorted,
`ensure_ascii=False` and separators `(',', ':')`. Input file digests use exact
UTF-8 bytes. Paths must have no absolute, `.`/`..` or hidden components. Do not
preseed a declared output. All source record criteria must refer to records
declared in that case; records in calibration are unavailable to held-out cases.

Save bundle.json, your generation/validation code, and rationale.md explaining
task coverage, ground-truth derivation, case distinctness, presentation equivalence
and foreseeable limitations. Validate counts, budgets, distinct identities,
source hashes, output predicates and input derivations yourself without evaluated
model inference. Final reply contains only counts and bundle file hash. The
coordinator independently validates the bundle before the first trial. A failed
preparation is retained; no replacement selection follows in this registration.

## Adjudication

Do not select new cases or change any existing artifact. Read the frozen protocol,
selection rationale/generator, clock ledger, replay summaries and model reports.
Independently spot-check ground truth from supplied facts and compare actual
output and source attribution against it. Inspect model-visible messages for
unassigned sources, missing task text, answer keys or lessons in withheld arms.
Report exactly which available model/family/arm samples you inspected. If a
planned population is absent, record it as missing, never as a passed control.

Check that each model used separate calibration, the same sealed definitions,
an observed digest, and no more than its registered active hour including setup,
shared preparation, scoring and your review. Note shared evaluator-model limits.
Verify artifacts are present and pinned; a receipt alone is insufficient.

For each family on each model: all trials and controls must exist; full minus
lessons-withheld completion must be at least .10 with a strictly positive lower
endpoint of the case-paired 95% bootstrap interval; false verified and budget
violations must both be zero; full-arm mean Brier must be below .25 with a nonempty
conclusive population. Missing evidence, disputed replay, expiry or changed pins
prevents technical qualification. Incomplete comparison metrics remain null.

Write review.md and review.json in your review directory. Your invocation supplies
the JSON fields. Separate acceptance of honestly incomplete evidence from technical
qualification. Preserve disagreements, unavailable checks, and methodological
limitations. Human adjudication is separate and pending. Always keep
`s6a_complete:false`; do not approve on Dhilip's behalf or enable any policy.
