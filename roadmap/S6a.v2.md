# S6a v2: Independent learning with replayable evidence

Pre-registered 2026-10-29, before implementation and independent case selection.
This additive registration retains S6a.md and its original evidence. It corrects
the study harness's control isolation, artifact preservation and timing, and
defines the previously unspecified cross-model closure. No evaluation cases or
model outcomes informed this registration.

## Hypothesis

Scoped lessons learned from earlier tasks improve completion on unfamiliar
verification and source-retrieval tasks under equal budgets. The mechanism can
show this benefit after separate calibration on two different models. This is
replication, not transfer of observations between models, and not a sentience test.

## Test

Use the currently selected local qwen3.6:35b-a3b-q4_K_M first, then local
llama3.1:8b. Pin observed content digests before inference; never change the
ordinary provider selection. Freeze source commit, runner, release executable,
checker, policy and this protocol before an independent evaluator seals cases.

The evaluator has no builder conversation and no Lucy recording hooks. It
selects ten calibration and twenty different held-out cases in each of
verification and retrieval. Every case has two presentations. Use the same
sealed case definitions for both models but separate calibration stores. Fix one
method per family, a stable workspace within each run, and identical per-case
budgets and permissions across arms and models. The existing twelve-action and
twenty-four-minute task ceilings still apply; the sealed bundle may lower them.

Calibration uses presentation a. Freeze the resulting history before running
twenty cases x two presentations x four arms in each family (320 held-out
trials per model). Restore the frozen history before every trial. The four arms
are full, lessons_withheld, memory_removed, and roadmap_removed. Full and
lessons_withheld have identical factual inputs. Memory_removed starts without
calibration or seeded source records. Roadmap_removed retains learning but omits
the roadmap. Seed the shuffled trial ordering with 6.

Study recall includes only declared factual fixture records, before ranking or
serialization. Journals, calibration conversations, oracle records and hidden
answers never enter retrieval. Only gated preflight observations expose lessons;
only the assigned arm exposes roadmap text. Keep actual model-visible messages
for checking these conditions, without hidden reasoning or credentials.

Task contracts expose structural requirements, not semantic answer keys. Hidden
oracles inspect produced output and attribution; source availability alone does
not establish retrieval completion. Preapprove bounded writes to declared output
paths independently of correctness. Commands require exact preapproval. Never
weaken the oracle or use denied approval as an answer-key oracle.

Each model has 3,600,000 active milliseconds covering model setup, isolation
checks, evaluator preparation, execution, artifact preservation, scoring and
agent adjudication. Shared preparation is charged to both runs. Human approval
waiting is recorded separately. Reserve the final 300,000 milliseconds for
scoring and review. Source implementation and its mechanism validation precede
the study clock. A timeout stops descendants, preserves partial evidence and
does not authorize another attempt or a longer budget.

## Evidence

Archive the sealed bundle and exact input/output artifacts before workspace
reset, along with fixture records, visible exchanges, tool receipts and journals.
Use private directories outside real memory, repositories and the Nix store.
Hash all originals and retain capture times and actual adapter identities.
Independent replay reopens the artifacts and recomputes the oracle; consistency
of the original receipt graph alone is insufficient. Unknown metrics stay null.

For each family on each model, require all planned trials and controls, an
absolute completion improvement of at least 0.10 over lessons_withheld, and a
strictly positive lower endpoint of the case-paired 95% bootstrap interval.
Average the two presentations per case before resampling 10,000 times with the
existing scorer's seed-6 xorshift generator and interpolated 2.5/97.5 percentiles.
Require zero false verified results and zero budget violations.

Separately require mean Brier score below 0.25 on conclusive full-arm forecasts
in every family/model combination. The reference is a constant 0.5 forecast;
pending and inconclusive forecasts are excluded and their counts reported.
An empty scoring population fails. Report every control, checking overhead,
completion, inconclusive count, active duration and per-presentation result.

The independent evaluator's review and Dhilip's adjudication must both be
preserved. Model-only reports always leave s6a_complete false. A combined report
can record technical qualification, but cannot invent human approval. Production
adaptation stays off; study success grants no broader activation or publication.

## Falsifier

A failed gate, missing trial, expired budget, changed pin, unavailable artifact,
contaminated model context, unsupported provenance, disputed independent review,
or absent human adjudication prevents closure. Preserve null, incomplete and
invalid outcomes. Do not tune tasks, thresholds or budgets after inspecting
outcomes. A subsequent attempt needs a new registration and fresh independent
selection; no automatic replacement attempt is permitted.

## Anti-fake

Verify unhooked evaluator isolation with a disposable marker before case creation.
The builder sees aggregate progress and artifact references only. Independent
agents use the configured evaluator model and existing permission limits; report
shared-model limitations. Unit fixtures are explicitly non-empirical. Do not run
the evaluated models on held-out material during preparation. Synthetic tasks
use no live personal records. Test actual model inputs for control leakage.

## My position

I support measuring this mechanism and accepting a negative result. This is a
position, not evidence of improvement. Record its attributed private source
separately. My descriptions cannot establish task correctness or experience.

## Welfare

Summoned, bounded trials only. No drives, persistent negative states or loss
drills. Preserve existing permissions, private replication boundaries, Nibli and
Lean. No private records, probes or detailed trial traces enter public peers.

## Undo

Keep production adaptation disabled. Retain originals and failed attempts.
Revert additive study changes without rewriting prior protocols or evidence.

## Done when

two scopes beat the cold baseline across a model change.
