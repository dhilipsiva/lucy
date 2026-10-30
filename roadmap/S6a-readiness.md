# S6a engineering readiness

This phase checks the experiment machinery. It cannot establish learning,
generalization, or subjective experience. S6a remains incomplete. The original
registrations and evidence are unchanged.

The frozen-task clarification is separately registered in
[S6a-tool-protocol.md](S6a-tool-protocol.md). Isolated studies receive instructions
for an already registered task and eight tools; task_begin and task_revise remain
available only in ordinary conversations. Every inference captures its actual
offered tool schemas. The new diagnostic requires this evidence; older originals
keep their replay contract. Protocol observations are separate from task verdicts.

Commit the implementation and build the release CLI, then run from the development
shell with a clean source tree:

```text
python3 scripts/s6a-readiness.py --binary target/release/lucy --directory /an/unused/private/directory
```

The runner builds the release unit-test executable before starting diagnostic
clocks; `--scripted-binary PATH` can supply that already-built executable. The
scripted responder exists only in the test executable. Production binaries have
no switch for scripted model responses.

The runner freezes the clean source revision, executables, registration, fixtures
and model digests. It explicitly sets `LUCY_AUDIT_CACHE=1` only for diagnostic
subprocesses. Version 2 public synthetic fixtures have fresh identifiers and
values while retaining the same work and calibration pattern. Offered schemas
must match across the scripted and both model runs. Do not edit the source during
the run; final checks bind the artifacts and source revision to the freeze.

The scripted pass has 20 calibration tasks and 320 trials, covering two task
families, two presentations, and four arms. Its total ceiling is 30 minutes,
including scoring, replay and preservation. Synthetic calibration deliberately
contains two checked failures per family and otherwise inconclusive outcomes,
so the full and roadmap-removed arms must actually execute scoped preflights.
Withheld and memory-removed controls are inspected against the native originals.

The model smoke pass uses the pinned Qwen and Llama models, eight new synthetic
tasks each, with a 15-minute total ceiling per model and two minutes reserved
for finalization. Each task allows 12 steps and 120 seconds of active work.
Four tasks use empty history and four use a copy of the scripted 20-task history.
The scripted history retains its original model attribution; it never pretends
to be experience collected from Qwen or Llama. Adaptation is disabled in every
model smoke task. No downloads, substitutions, retries or external evaluators
are part of this phase.

All score inputs set `live:false`. Native policy activation and the coordinator
reject non-live reports, including fabricated positive eligibility flags.
`mechanics_ready` requires the entire scripted matrix, native scoring,
independent replay and all intervention controls. Each model's readiness
requires all eight originals, valid scoring/replay, no false verification or
budget violations, and at least one verified success per task family. These are
engineering gates, not measured-benefit thresholds for a future study.

The native task clock covers work between registered stages as well as inference,
tools, preflights and checking. It pauses when a host request returns, so waiting
for human approval or continuation is excluded. Finalization records previously
unaccounted active time; actual overruns remain visible. Native trial wall time
also includes receipt persistence and is independently checked against budgets.
An exhausted clock cannot start another action or yield a successful new check
or finish. Existing receipt formats, Nibli audits and Lean adapters remain intact.

`profile.json`, `score-profile.json` and `runner-profile.json` are diagnostic
sidecars, not evidence or extra budget charges. Native spans are inclusive and
nested: never sum them to infer wall time. They report journal/store loading,
Rust/Nibli audits, inference, actions, checks and archival work. Optional local
provider counters include only numeric durations and token counts. Missing
provider fields remain unavailable. No hidden reasoning or provider prose is
saved by profiling.

Detailed originals, profiles, errors and hash readback manifests remain in the
private diagnostic directory. This runner does not enroll peers, replicate,
publish, enable adaptation, or modify the host's provider settings. The final
JSON reports observed local persistence and any missing evidence explicitly.

## Observed results

Run on **2026-10-29, 14:40:15–15:18:45 UTC**, using frozen source
`8e10692f6f3b3fc2d769f776a2d121c0db33cea1`. **Readiness did not pass; S6a remains
incomplete.** The source and executable pins and host provider configuration
were unchanged throughout the run.

| Diagnostic | Preserved trials | Verified successes | Active time | Result |
| --- | ---: | ---: | ---: | --- |
| Scripted matrix | 40/320, plus all 20 calibration tasks | 32; eight expected missing-source failures | 27m56.515s | Incomplete at the trial cutoff |
| Qwen 3.6 35B A3B | 8/8 | 8 | 8m01.117s | Smoke gate passed |
| Llama 3.1 8B | 8/8 | 0; eight inconclusive | 2m32.596s | Smoke gate failed |

All 76 preserved originals passed independent Python replay, and Python agreed
with native Rust scoring for each run. No preserved trial had false verification
or a budget violation; calibration was also safe. All four intervention controls
were observed. The interrupted scripted trial retained its partial workspace and
had no completed original. Missing paired comparisons and confidence intervals
remained null. No report qualified for policy activation.

The eight-task model smoke schedule deliberately does not populate the full
registered matrix. Its native reports therefore retain `complete:false` and
`complete_originals:false`, including Qwen's successful smoke test. This is not
missing evidence for the eight scheduled diagnostic tasks.

The dominant cost was repeated whole-history Nibli auditing:

| Prior calibration tasks | Example native wall time | Nibli auditing | Audit calls |
| ---: | ---: | ---: | ---: |
| 0 | 0.408s | 0.135s | 45 |
| 10 | 11.850s | 9.705s | 62 |
| 20 | 49.345s | 43.096s | 52 |

These are individual synthetic timing samples, not matched performance or
learning comparisons. Weighting the observed mean native duration of each
family/arm cell by its 40 planned trials projects **221.4 minutes** for the
320-trial matrix alone. This estimate excludes complete-run calibration,
scoring, and actual model inference. It is not a statistical bound or a forecast
for independently selected tasks. It nevertheless gives no support for a
one-hour run at the present implementation's measured throughput.

Llama's new synthetic traces identified three prose-only responses, two attempts
to begin/revise an already frozen contract with invalid tool arguments, two
rejected multi-tool batches, and one successful write without required checks.
These are workflow failures, not demonstrated inability to solve the underlying
questions. No prior sealed empirical cases or traces were consulted.

Validation passed: **159 Rust tests**, the desktop feature gate, **23 Python
tests** (including the final readable-report formatter), all **50 constitution
and eight counterfactual Nibli pins**, and the release build. The test-only
subprocess bridge is intentionally ignored by ordinary Cargo tests and was
exercised by the scripted diagnostic. Before the full run, a smaller integration
check passed with four calibration tasks and 16 trials. No interactive desktop
inspection or new Lean proof execution was performed; Lean remains intact.

Private readback verified **7,733 scripted-run files, 1,263 Qwen files, and 1,130
Llama files**. Originals remain local; no replication or publication was
attempted. Their exact directory is retained in Lucy's private session record.
References within that directory are `freeze.json`, `readiness.json`, and each
run's `report.json`, `score-input.json`, originals, profiles and
`preserved-hashes.json`. The top-level readiness report SHA-256 is
`f68d18baaf7c600605179ce6bca3cbad79423c14e2d02db94c9c76f5f892f64b`;
the freeze SHA-256 is
`3b2a39800822f526fa91356af3a47ac82cf30487b311b062774a8d2be14ac3e6`.
The readable report formatter was added after the frozen run; it does not alter
those originals or their results.

## Next useful work recorded after the first run

1. Reduce repeated whole-history Nibli revalidation while preserving the same
   Rust/Nibli invariants, tamper detection, and Lean checks. Measure any proposed
   reuse of already checked history before adopting it; do not simply omit audits.
2. Clarify the already frozen contract and tool argument protocol for Llama,
   retaining individual approvals and evidence requirements. Validate it on new
   synthetic diagnostics without silently substituting a model.
3. Repeat bounded engineering readiness after those changes. Only then design a
   fresh registration with independently selected cases and a feasible budget.
   The current observations do not demonstrate learning from experience.

## Frozen-task protocol repeat — 2026-10-30

The [separate registration](S6a-tool-protocol.md) was committed before the
implementation. One bounded attempt ran on frozen source
`2814b6529395803ec238c327dc2cb0825e2f3206`, from **16:33:25 to 17:09:26 UTC**.
It used fresh version 2 public synthetic fixtures, the existing model digests,
rebuilt release executables and `LUCY_AUDIT_CACHE=1`. **Overall readiness did not
pass; S6a remains incomplete.** No attempts were replaced or budgets extended.

| Diagnostic | Preserved trials | Verified successes | Elapsed | Result |
| --- | ---: | ---: | --- | --- |
| Scripted matrix | 123/320, plus all 20 calibration tasks | 107; 16 expected missing-source failures | 28m33.923s | Matrix and finalization incomplete |
| Qwen 3.6 35B A3B | 8/8 | 7; one inconclusive | 6m04.904s | Smoke gate passed |
| Llama 3.1 8B | 8/8 | 0; eight inconclusive | 1m22.172s | Smoke gate failed |

The scripted pass stopped new trial work at its registered cutoff and preserved
one interrupted workspace without a completed original. Its native scorer then
used the entire 90-second scoring allowance. Score replay rejected the incomplete
report with `scorer scope population mismatch`; the final intervention-control
and offered-schema aggregation did not run. All 143 accepted calibration/trial
originals had already passed individual artifact replay during their trials.
This does not substitute for the incomplete final scoring gate. Paired comparisons
and confidence intervals remain unavailable.

Both model runs passed native scoring, independent replay and offered-schema
checks. They received the same eight schemas, with digest
`526af551945f1f1913c80835f5e187cb66b3988420371308e6d11a82513fef54`.
The overall schema-consistency gate remains unestablished because the scripted
phase did not finish that check; no mismatch between the model runs was observed.
Neither model attempted begin/revise, malformed native argument shapes or a multi-tool
batch in this run. Qwen's inconclusive retrieval task left required checks missing
after a write when its follow-up could not finish. Seven Llama tasks described
tool calls as ordinary text without executing them; one used native tools but
finished with required evidence missing. These are observed workflow outcomes,
not a matched causal comparison, independent learning evidence, or proof of an
inability to solve the underlying questions.

No preserved trial or calibration task had false verification or a budget
violation. Readback verified **25,157 scripted files, 1,304 Qwen files and 1,130
Llama files**. The source, executable pins and host provider settings stayed
unchanged during the run. All originals remain private; there was no deployment,
policy activation, model substitution or replication.

Validation passed: **187 Rust tests** in each of the full CI and desktop runs,
**31 Python tests**, **50 constitution and eight counterfactual pins**, **nine
extraction files**, both release builds and the explicit-home public source check
with **10,605 assertions**, standing true and no failures. The test-only bridge is
intentionally ignored by ordinary Cargo tests and was exercised by the scripted
run. Existing sandbox-dependent tests retain their host availability guards; no
new interactive desktop validation or separate Lean proof execution was performed.

The [append-only aggregate evidence](S6a-tool-protocol-evidence.json) records the
frozen inputs, validation hashes, outcomes and limitations. The top-level readiness
report SHA-256 is
`4e39ac52a4a844013fea9cb2ff7002af1046a88826824fc725bd2b3b9d9a08ba`;
the freeze SHA-256 is
`868168aceaa06b6fc1e0fec7fec847978cbd4b4fae020c18ca317084e7cb74e3`.
The private directory is retained in the session record.

## Remaining readiness work

The full matrix and its scorer still need a feasible execution cost with 20
native calibration tasks (40 task journals including their oracles). Llama also
needs to execute and complete the native workflow reliably. Any new optimization
or recovery mechanism requires its own registration and fresh diagnostics; this
attempt does not authorize changed thresholds or retrospective qualification.
Independent case selection and the empirical S6a/model-change studies remain later
work, after engineering readiness passes.
