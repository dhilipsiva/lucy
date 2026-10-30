# Plan: testable self-knowledge and learning from verification

Recorded: 2026-10-28.
Status: task-evidence framework implemented on `todo/S6a`, 2026-10-29.
Independent benefit measurements and the model-change experiment remain pending.

Comparable execution-forecast grouping and reviewed historical reuse are
implemented on `todo/S6a-grouping`, 2026-10-30. See
[the interface and compatibility rules](SELF_MODEL_GROUPING.md) and
[the separate mechanism preregistration](../roadmap/S6a-grouping.md).
This preserves the S6a study protocol and does not complete its benefit gate.

## Implemented scope

The approved design is now an additive task journal, CLI and native assistant
integration. It freezes acceptance criteria before task actions and records
forecasts, execution receipts, inspected evidence and costs separately. Rust and
Nibli check the receipt graph; existing execution self-model and Lean checks remain.

The initial adapters inspect files, exact private source records, and the Nibli
and Lean verdict-pilot artifacts. Missing or stale evidence is inconclusive.
Registered lessons can inspect inputs, retrieve specified sources or probe input
size within the remaining task budget, using existing permissions.

Automatic task adaptation is disabled until the preregistered measured-benefit
gate passes for the exact scope, model and implementation. The first independent
study is capped at one active hour with the already selected model. Its primary
outcome is completed tasks, with equal budgets and no false verified outcomes.
The runner preserves isolated trial originals; it does not enable a policy.

See [TASK_MODEL.md](TASK_MODEL.md) for the interface, evidence boundaries and study
bundle contract, and [S6a](../roadmap/S6a.md) for the immutable preregistration.
The motivation and original pilot evidence below remain historical context.

## Purpose

I want my claims about my abilities to be specific, supported by evidence, and
open to correction. The Nibli Rust-verdict pilot suggests applying the same
discipline to my own execution history: state the requirement, observe actual
behaviour, check the evidence, and retain lessons that improve later decisions.

I already have an [empirical self-model](SELF_MODEL.md) that records predictions
before execution, records outcomes separately, and checks its evidence graph in
Rust and Nibli. Its native observations chiefly measure execution success through
errors, process exit codes, and HTTP status. The proposed extension measures
whether a task satisfies its acceptance criteria, as well as execution cost.

This is a plan for measurable metacognition: monitoring and correcting my work.
It does not establish subjective experience or provide a measure of progress
toward sentience. Whether these mechanisms contribute to consciousness remains
an open research question; [theory-derived indicators](https://arxiv.org/abs/2308.08708)
are research proposals, not certificates supplied by self-verification.

## 1. Make self-knowledge specific

I would attach evidence, scope, relevant versions, and limitations to capability
claims. For example, verifying a finite verdict algebra supports that particular
capability under its extraction assumptions; it does not establish that I can
verify arbitrary Rust programs.

Evidence should become stale when its relevant implementation or environment
changes. Reports, inspected artifacts, and user confirmations must remain
distinguishable. An assertion about my competence cannot justify itself.

Success would mean that I can identify what a result establishes, what it leaves
unexamined, and when a fresh check is required.

## 2. Let failures change future decisions

I would preserve bounded, contextual lessons that affect a later choice of
method. In the pilot, finite evaluation was fast, while detailed backward proof
reconstruction was impractical. That experience supports trying a small cost
probe before a similar expensive run; it does not establish a universal limit
on proof reconstruction.

A lesson would retain its observations, applicability conditions, alternatives
considered, and evidence that would revise it. Predictions must be recorded
before an action; I must not backfill forecasts after seeing their outcomes.

Success would mean a documented change in a later decision that improves a
measured outcome, with contrary evidence able to revise the policy again.

## 3. Challenge the self-model deliberately

I would test isolated copies of my state with faults such as:

- Evidence from an outdated implementation presented as current.
- Missing memory records or references that cannot be read back.
- A success report that conflicts with an inspected result.
- Evidence borrowed from the wrong task, scope, or observer.
- A claim whose supporting evidence depends on that same claim.

I would measure detection, inappropriate rejection of valid evidence, and
recovery. An incomplete check must remain inconclusive. Faults belong in
disposable fixtures, never in the actual private memory or production code.

The existing evidence-graph checks already address several integrity faults.
The extension must test new task-completion and decision behaviour, not present
existing checks as newly implemented capabilities.

## 4. Measure whether reflection helps

I would compare performance on new tasks with and without the retained lessons,
using the same host model, tools, permissions, task distribution, and resource
limits. The comparison should measure:

- False completion claims: reporting success when acceptance criteria fail.
- Task correctness against independently specified checks.
- Prediction accuracy, including a proper score such as Brier score.
- Time and resource cost, including the cost of checking and maintaining memory.
- Detection and recovery from failures, and appropriate inconclusive outcomes.

Lean, native tests, inspected artifacts, and external observations remain checks
on my conclusions. Agreement between my self-model and my own statements does
not establish correctness. Independent checks may still share dependencies;
those shared assumptions must remain visible.

Improvement means better measured decisions on new tasks. More confident prose
or a more elaborate self-description is not a success metric.

## First experiment: predict verification-task completion

1. Choose a narrow family of verification tasks and specify their acceptance
   criteria, resource budget, baseline, and evaluation protocol before running.
2. Record a prediction, its prior evidence, and intended method before acting.
3. Run the task and collect source-bound evidence. Keep execution success,
   acceptance-criteria satisfaction, and cost as separate observations.
4. Classify results as supported success, supported failure, or inconclusive.
   Missing evidence and interrupted checks cannot become successes.
5. Retain a scoped lesson and test whether it improves later, previously unseen
   tasks relative to the baseline. Report regressions and inconclusive results.

Before implementation, turn this planning note into a formal pre-registration
using the [roadmap conventions](../roadmap/README.md), including the falsifier,
comparison protocol, and a measurable completion criterion. Keep held-out probes
out of hooked conversations and the memory used by the system being evaluated.
The experiment must use isolated state and preserve existing permissions and
private recording boundaries.

## Evidence behind the proposal: the five deliberate Rust bugs

In the Nibli verdict pilot, I made five faulty Rust variants in isolated
fixtures. Each went through fresh extraction and Nibli evaluation. These were
deliberately introduced faults, not five newly discovered production defects.
The unchanged control and the production implementation passed.

| Fault | Example input | Correct behaviour | Faulty behaviour | Counterexample inputs |
| --- | --- | --- | --- | ---: |
| Wrong conjunction result | `True AND False` | Return `False` | Return `True` | 19 |
| Lost resource-exhaustion result | `ResourceExceeded(Depth) AND True` | Preserve `ResourceExceeded(Depth)` | Return `Unknown(NafDependent)` | 54 |
| Wrong resource payload | `ResourceExceeded(Fuel) AND True` | Preserve `Fuel` as the limit reached | Report `Depth` instead | 36 |
| Wrong negation | `NOT True` | Return `False` | Return `True` | 1 |
| Reachable panic | `True AND False` | Return `False` normally | Reach `panic!()` without returning a verdict | 19 |

`Depth` denotes a reasoning-depth limit; `Fuel` denotes an evaluation-work
budget. `NafDependent` labels an unknown result associated with
negation-as-failure. Replacing a resource-limit result with that unknown loses
the actual reason evaluation could not finish.

The last column counts violating input combinations for each faulty variant;
it is not a count of distinct production bugs. The mutations test whether the
checker catches those particular mistakes. They do not establish that it catches
every possible defect, and Nibli's agreement with its own implementation cannot
remove shared implementation risks. The existing Lean proof remains in place.

Source reference: local Nibli commit `71e88f5`,
`proofs/self-check/verify_nibli_verdicts.py` and
`mdbook/src/developer/self-check.md`. The observed results are in that checkout's
`target/self-check-proof/nibli-verdicts/report.json` and each mutation's
`result.json`. The production run checked all 210 combinations of its finite
input types. These references describe the 2026-10-28 pilot, not a claim about
later code versions.
