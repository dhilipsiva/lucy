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

Pending the bounded readiness run. Results will distinguish harness correctness,
model limitations and runtime feasibility. Any extrapolation to a one-hour study
will be labelled an estimate from synthetic tasks, not a learning result.
