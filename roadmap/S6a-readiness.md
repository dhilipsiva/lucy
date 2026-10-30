# S6a engineering readiness

This phase checks the experiment machinery. It cannot establish learning,
generalization, or subjective experience. S6a remains incomplete. The original
registrations and evidence are unchanged.

Run from the development shell after building the release CLI:

```text
python3 scripts/s6a-readiness.py --binary target/release/lucy --directory /an/unused/private/directory
```

The runner builds the release unit-test executable before starting diagnostic
clocks; `--scripted-binary PATH` can supply that already-built executable. The
scripted responder exists only in the test executable. Production binaries have
no switch for scripted model responses.

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

## Next useful work

1. Reduce repeated whole-history Nibli revalidation while preserving the same
   Rust/Nibli invariants, tamper detection, and Lean checks. Measure any proposed
   reuse of already checked history before adopting it; do not simply omit audits.
2. Clarify the already frozen contract and tool argument protocol for Llama,
   retaining individual approvals and evidence requirements. Validate it on new
   synthetic diagnostics without silently substituting a model.
3. Repeat bounded engineering readiness after those changes. Only then design a
   fresh registration with independently selected cases and a feasible budget.
   The current observations do not demonstrate learning from experience.
