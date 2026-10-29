# Task evidence and bounded learning

The task model records whether fixed acceptance criteria were satisfied. A
successful command is an execution observation; it cannot establish completion
on its own. This adds to [the execution self-model](SELF_MODEL.md), without
changing its record format or removing the Nibli/Lean verification pilot.

Implementation: `src/task_model/`, `src/desktop/tasks.rs` and
`constitution/task_model.nibli`. Research protocol: [S6a](../roadmap/S6a.md).
Mechanism tests are not evidence of improved performance or consciousness.

## Native assistant

For tool work, Lucy drafts criteria from the request and calls `task_begin`
before taking an action. The contract and forecast are persisted first. A
material ambiguity can still be clarified with the user. Existing write,
command, network and desktop approvals remain in effect.

`task_check` selects a frozen criterion by ID. Callers cannot provide its verdict.
`task_finish` combines the latest inspected evidence and checks its freshness.
The terminal and desktop show a separate evidence status alongside the response.
Useful output can remain available when verification is inconclusive. There is
no required retry loop. Changing the request requires a subsequent user turn
and `task_revise`, which preserves the earlier attempt.

The default ceiling is twelve action/check steps and twenty-four active minutes.
A contract can lower either limit. Model inference, registered preflights,
execution and checking are charged; waiting for human approval is not. The
existing twelve-response loop and per-command timeout still apply. Cancellation,
unresolved execution, missing evidence or exhausted time cannot become success.

## Private journal and trust boundaries

Records use the existing private store under `lucy:task-model:v1`. Each entry has
a capture timestamp, content digest, predecessor and sequence. The record is
read back after saving. The beginning freezes the criteria, observer, actual
adapter source/session, workspace digest, method, model/environment, forecast,
checker version and lesson candidates. A CLI observation cannot resolve a
native task. Recording a second identical execution receipt is idempotent;
conflicting receipts are rejected. Do not repeat an executed action to repair a
failed recording.

Evidence distinguishes observed predicates, inspected imported artifacts,
reported statements and user confirmations. The last two cannot independently
establish success. Original reports stay at their source: the journal retains
their paths and hashes, not a complete copy of every artifact. `replay_available`
means sources were readable at capture time. Replaying later requires those
originals. Source deletion or edits leave earlier receipts intact.

Private task data uses existing private replication classification. No new peer,
publication destination or permission is created. Study originals stay in their
own throwaway store. Unknown resource metrics are JSON `null`.

Rust and Nibli independently check graph integrity, including missing, future,
cross-scope, cross-task and cyclic support. Rust additionally validates schemas,
hash chains, criteria conclusions and budgets. Nibli uses the pinned corpus's
`sl_*` predicates and typed `exist` relations in a separate engine instance; no
dependency pin or vocabulary change is required. This checks supplied evidence
relationships, not the truth of an arbitrary report. Both share the Rust fact
encoder and the Nibli implementation; Lean remains a separate external check.

## Evidence adapters

| `check.kind` | Required fields | What it establishes |
| --- | --- | --- |
| `file_exists` | `path` | A readable regular workspace file exists; its digest is retained. |
| `file_sha256` | `path`, `sha256` | Its bytes match the fixed expected digest. |
| `json_equals` | `path`, `pointer`, `expected` | A JSON pointer equals the fixed expected value. |
| `record` | `origin`, `record`, `sha256` | An exact source record is retrievable with the expected digest. |
| `nibli_report` | `path` | The finite verdict-pilot report, case results, source inventory, extracted artifact and binaries agree. |
| `lean_report` | `path` | The verdict-pilot source/extraction manifest, successful build/proof receipts, logs and axiom audit agree. |
| `reported` | `statement` | An explicit manual requirement; remains inconclusive. |

Paths are relative to the frozen workspace and use native path restrictions.
JSON is bounded to 32 MiB and file hashing to 256 MiB. An unreadable or unsupported
artifact is inconclusive. Finalization rechecks hashes and proof source
inventories, including newly added source files. Imported reports are labelled
**inspected reports**, never fresh Nibli executions or Lean kernel runs. The
verdict-pilot adapter does not establish equivalence of the full self-checker or
arbitrary Rust code.

## CLI

All mutations take JSON through stdin. Use `--root` for an existing authorized
private store and `--workspace` for the task directory; their defaults are the
ordinary private store and current directory. Never put private records in Git.

Example definition for `lucy self-model task begin --stdin`:

```json
{
  "source": "your-actual-adapter",
  "session": "your-actual-session",
  "title": "Produce the requested JSON artifact",
  "family": "verification",
  "method": "generate-and-inspect",
  "model": "your-actual-model-and-revision",
  "environment": "your-recorded-environment",
  "criteria": [{
    "id": "ready",
    "description": "result.json has ready=true",
    "check": {"kind": "json_equals", "path": "result.json", "pointer": "/ready", "expected": true}
  }],
  "budget": {"steps": 6, "active_ms": 120000}
}
```

Retain the returned task ID. Subsequent inputs are:

```text
run:    {"task":"ID","argv":["an-explicitly-authorized-command","argument"]}
check:  {"task":"ID","criterion":"ready"}
finish: {"task":"ID","interrupted":false}
revise: {"task":"ID","definition":{...new complete definition...}}
status: {"task":"ID"}
```

`run` executes the host's explicit argv, with bounded output and time, and records
execution separately. It does not serve as a sandbox bypass for native tools.
The native assistant always uses its existing approval and sandbox path.
`status`, `events`, `facts` and `audit` inspect the private journal; these do not
publish it. `status --stdin` can select one task; without stdin it lists tasks.

## Lessons and activation

Forecasts use conclusive outcomes among the last 32 finalized tasks in the exact
scope, with a Beta(1,1) prior. Two failures exceeding successes can nominate a
registered lesson. Successes remain contrary evidence and can deactivate it.
The scope includes source, observer, workspace, family, method, model and
environment; changing any of these starts a separate history.

Only three templates exist: inspect required input metadata, retrieve specified
source records, and probe input size before expensive work. The cost probe is a
bounded size/availability inspection, not an estimate from a proof execution.
Templates use remaining steps/time and never run instructions from memory,
enlarge a budget or grant permission. Candidates are withheld from native model
tool replies; only the gated preflight introduces their observations.

Automatic task adaptation starts **off**. Enabling requires a preserved eligible
study for the exact scope, protocol, model, checker, policy and executable digest.
A missing study, changed binary or explicit disable leaves adaptation off.

```text
lucy self-model task policy --stdin
{"scope":"TESTED_SCOPE_DIGEST","enabled":true,"study":"SCORED_REPORT_ID"}

lucy self-model task policy --stdin
{"scope":"TESTED_SCOPE_DIGEST","enabled":false,"study":null}
```

Disabling retains the observations. This gate is separate from the older
execution model's existing directory-inspection policy.

## Independent study runner

Build the final binary, obtain its identity with
`lucy self-model task study-info`, and freeze the implementation commit. An
independent evaluator then prepares a private bundle. Do not open held-out
material in the building agent's hooked conversation. The bundle contains:

```text
schema: 1
evaluator, evaluator_model: accountable evaluator identities
implementation, protocol: digests from study-info
calibration: 10 cases in each of verification and retrieval
held_out: 20 different cases in each of those same scopes

Each case:
  id: unique across both sets
  disguises: {a: prompt, b: independently worded prompt}
  definition: complete frozen Definition (source/session/model/environment
              are replaced by the actual native adapter identity)
  oracle: independent Criterion array
  files: optional map of ordinary relative paths to UTF-8 content
  records: optional private fixture:... Record array
  approvals: optional exact {tool, arguments} requests already authorized
             by the study operator; other proposals are denied
```

The evaluator is responsible for meaningful criteria, sealed case selection and
independence. Local digests preserve attribution and detect changes; they do not
cryptographically prove an evaluator's independence. Unit fixtures never count
as measured benefit. A requested model name without an observed revision stays
`revision-unknown` and cannot activate a policy.

```sh
python3 scripts/self-model-study.py --bundle /private/path/sealed-bundle.json \
  --directory /private/path/new-study --seconds 3600
```

The runner refuses repository-backed, existing or real memory directories and
overlapping identity/workspace/store paths. It preserves the selected provider
and approvals, disables peer discovery/sync, and uses isolated identity and data.
Calibration is frozen before held-out trials; each trial restores that snapshot.
The memory-removed arm starts empty. Full and lessons-withheld use identical
facts, criteria and budgets; only gated templates differ. Roadmap-removed keeps
adaptation but omits roadmap text. These roadmap inputs are exclusive to the
explicit study path, never ordinary wake or exported training data.

There are 320 held-out trials: two scopes × twenty cases × two disguises × four
arms. Results are clustered by case for the paired bootstrap (10,000 resamples,
seed 6). The scorer reopens both calibration and held-out originals and their
oracle journals. All planned trials, a completion gain of at least 0.10, a paired
95% lower endpoint above zero, and no false verified/budget violations are
required. Reports include completion, inconclusives, Brier scores and costs.

The active-hour ceiling includes preparation, model work and adjudication. A
timeout kills the trial process group; partial originals remain private. A
short post-timeout diagnostic pass can emit an explicitly incomplete report,
which cannot activate anything. Reports never enable a policy automatically.
S6a remains open until independent adjudication and the separate model-change
requirement are met.
