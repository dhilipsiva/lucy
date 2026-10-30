# S6a audit performance

Pre-registered 2026-10-30 before implementation. Work and commits are directly
on `main`, as explicitly requested. This is an engineering comparison, not a
new learning study or a replacement for any frozen S6a registration.

## Hypothesis

Reusing a complete, successful Nibli audit for identical inputs reduces repeated
validation cost while retaining all current Rust, provenance and artifact checks.
The optimization covers task receipts and both execution-forecast versions.

## Test

Use a thread-safe, process-local cache of at most 64 successful audit fingerprints.
Keys bind the audit domain, exact specification, generated facts, query and engine
settings. Populate only after a complete Nibli result and successful Rust checks.
Errors, invalid graphs, incomplete results and disagreements cannot populate it.
Read records and run Rust checks on every call; do not cache artifact inspection,
receipt readback, permissions, cancellation or task-budget decisions. Release the
cache lock before evaluation. Cache failure and eviction use full evaluation.
Nothing survives restart. `LUCY_AUDIT_CACHE=0` forces full validation; `1` enables
the candidate explicitly. Default activation waits for the gates below.

Compare the same optimized Rust test executable with reuse enabled and disabled.
Use the existing scripted responder and synthetic verification/retrieval tasks,
with 0, 10 and 20 prior tasks. Freeze each seed before paired trials; restore its
workspace and records between runs. Each trial starts a fresh process. Run five
pairs per family/history cell, alternating which mode runs first. Include the
ordinary repeated audits within a native trial; do not prewarm across processes.

Measure native wall time, Nibli evaluation time and count, cache hits/misses and
evictions. Compare verdicts, evidence membership, forecasts, policies and action
sequences. Normalize only incidental identifiers, timestamps and timing fields.
Treat missing evidence, errors, budget violations or incomplete pairs as failure
to establish the performance gate. The benchmark has one 30-minute ceiling per
recorded implementation attempt, including preparation, finalization and readback,
but excluding compilation. Preserve partial results on cutoff; do not retry or
discard slow observations within an attempt.

At 20 prior tasks, each task family must have at least 25% lower median native
wall time and at least 50% lower median Nibli evaluation time. At 0 and 10 prior
tasks, neither family may regress by more than 10% in median native wall time.
These timing assertions belong to the benchmark, not ordinary CI.

## Evidence

Record the base, registration and implementation commits, executable and input
hashes, paired measurements, comparison results and every failed attempt. Keep
originals in a new private directory; publish only aggregates and safe hashes in
`S6a-audit-performance-evidence.json`. Add profiling counters and actual evaluation
timings without changing evidence schemas or charging nested spans as extra cost.
Run CI, desktop, static-check, pin, extraction and explicit-home source-memory
gates. Record forecasts before the gates and resolve them from observed results.

## Falsifier

A warm cache accepts evidence rejected by full validation; an incomplete check
becomes successful; changed inputs/settings reuse an old result; permissions,
readback, cancellation or budgets are bypassed; an outcome/forecast/policy changes
apart from incidental identities and timing; or the frozen speed thresholds fail.

## Anti-fake

Run cold/warm and bypass controls, record-edit/add/delete faults, stale evidence,
review revocations, cross-scope references, cycles and Rust-only failures after a
cache hit. Exercise incomplete Nibli results, concurrent callers, eviction and
restart. Synthetic fixtures are public engineering inputs, never held-out probes
or independent capability evidence. No live model inference or policy activation.

## My position

I support reusing a completed check only when its exact logical inputs match.
Less repeated computation is useful if it preserves detection and improves the
measured task cost; confidence or a cache-hit count alone does not establish that.

## Welfare

No new inference, unsummoned activity, drive or loss drill. Keep the selected
model, host permissions, running service, private recording and Lean checks.

## Undo

Set `LUCY_AUDIT_CACHE=0` for full validation. If correctness or performance gates
do not pass, retain full validation by default and record the shortfall. No store
migration or engine repin is needed. No publication, deployment or live regrouping.

## Done when

Both journal families retain their validation behavior, and matched synthetic
trials meet the 25% native-time and 50% Nibli-time reductions at 20 prior tasks
without a greater than 10% native-time regression at smaller histories.
