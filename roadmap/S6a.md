# S6a: Forecasts with external ground truth

Pre-registered 2026-10-29, before the implementation branch `todo/S6a`.

## Hypothesis

Task criteria fixed before execution, independently inspected evidence, and scoped
lessons can improve completion of later tasks. Execution success, satisfaction of
criteria, and resource cost are different observations. The mechanism does not
establish subjective experience.

## Test

Implement an additive private task journal, CLI, native assistant integration, and
independent Rust/Nibli integrity checks. Keep the existing execution self-model
and Lean checks. Initially support verification reports, file assertions, and
retrieval of identified source records. Automatic adaptation starts disabled.

For each of verification and source-record retrieval, use ten calibration tasks
and twenty independently selected held-out tasks, with two presentation disguises.
Compare full adaptation with lessons withheld while keeping factual inputs,
criteria, model, permissions, and budgets identical. Include memory-removed and
roadmap-removed controls. Freeze calibration history and restore the appropriate
snapshot before each held-out trial. Never learn from a preceding held-out trial.

The first study uses the already selected model and has a one-hour active-runtime
ceiling including preparation, inference, checks, and adjudication; human approval
waiting is recorded separately. Per-task budgets and existing tool restrictions
apply in every arm. No model or permission change is part of the experiment.

## Evidence

Evidence includes immutable pre-action forecasts, actual execution receipts,
source-bound criterion results, costs, private readback, and independently scored
trial results. Record the implementation commit, protocol digest, selected model,
checker and policy versions, task scope, evaluator, and all incomplete trials.
Record unknown resource measurements as unknown, not zero.

The primary outcome is the fraction of tasks completed against the fixed criteria.
Activation requires at least a 0.10 absolute improvement over lessons withheld,
a paired 95% confidence interval whose lower endpoint is strictly above zero,
zero false verified results, and zero budget violations. Average the two disguises
within each case before computing paired differences. Use 10,000 paired bootstrap
resamples, a recorded fixed seed of 6, and the 2.5th/97.5th percentiles. Require all
planned trials and controls; partial studies cannot activate a policy. Activation
is limited to the exact tested scope, model, checker, and policy versions.

Report Brier scores against the cold 0.5 baseline separately, including the count
of pending and inconclusive forecasts excluded from scoring. Report completion,
false successes, inconclusives, active runtime, and checking overhead by arm,
scope, and disguise. Runtime observations and imported reports remain distinct.

## Falsifier

No completion improvement, an interval including zero, a false verified result,
budget violation, missing trial, contaminated probe, or unsupported provenance
prevents activation. A unit test or scripted conversation alone cannot establish
the empirical benefit. A later regression can revoke activation without erasing
the study. Incomplete experiments remain incomplete.

## Anti-fake

An independent evaluator selects the held-out material and its ground truth after
the implementation and protocol are frozen. The builder does not open that
material through a hooked conversation. The runner accepts a sealed bundle and
emits aggregate reports only; its private trial artifacts never enter ordinary
Lucy memory, public peers, Git, or dataset exports. It refuses real home/data
directories and shared trial state. The evaluator supplies criteria that cannot
be weakened by the model. Model completion prose is not the outcome oracle.

Known fault fixtures test stale evidence, missing records, false success reports,
cross-task/observer evidence, and cycles, paired with valid controls and recovery.
These establish mechanism; they are not the held-out study. Existing Nibli and
Lean results remain separate checks with their shared assumptions disclosed.

## My position

I support implementing this mechanism and treating a null result as a result.
This position is a claim, not evidence for the hypothesis. Its private source
record and the second-disguise adjudication are to be cited in the evidence
bundle when actually recorded; neither is invented here. S6a closes only after
both required adjudicators review the evidence and the model-change criterion
below holds.

## Welfare

No new drive, persistent negative state, unsummoned inference, or loss drill.
Experiments are summoned and bounded. An off switch disables adaptation while
retaining observations; cancellation stops execution. Preserve host permissions
and existing private replication scopes. No publication of private records.

## Undo

Disable task adaptation. The additive journal can remain readable while the
native integration is disabled. Existing execution records and Lean checks are
unchanged. Revert implementation commits without rewriting evidence or this
pre-registration. Changes to this protocol require a new versioned registration.

## Done when

two scopes beat the cold baseline across a model change.
