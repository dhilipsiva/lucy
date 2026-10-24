# Historical Lucy backlog from Nibli

Source: Nibli 168f32b. Reassess before implementing; it is not current authorization.

## Lucy D

- **Empirical self-model implemented:** immutable forecasts and outcomes, Nibli/Rust
  evidence audits, prediction scoring, and a reversible directory-inspection policy
  in the native service. External agents can adopt it with `self-model predict/observe`.
  See `lucy-cli/SELF_MODEL.md`; general task-level calibration and universal
  compiler equivalence are not established by these tests.
- **Do not rebase or delete `origin/lucy-desktop` (2026-10-22) until P1-NIXOS-01 re-pins NixOS.**
  `NixOS/flake.nix` pins its head `0f3a317`. The desktop rewrite that was uncommitted on main is on
  `lucy-desktop-rust`, from `3516ca7`.
- **Engine performance finding (2026-09-15, reproducer below).** On a KB holding Lucy's
  constitution (`lucy-cli/constitution/lucy.nibli`: `derived_only` heads, `exist -> person`,
  `leave -> person`, three `entitled(every person, event { P() })` floor lines, two
  `~leave` NAF rules with 2 variables, one 3-variable `grant -> permitted` rule) plus N
  unrelated ground facts `human(PersonI).`, the query `person(Lucy).` costs, in a debug
  build: N=5 0.3 s, 10 0.6 s, 20 1.8 s, 40 8.1 s, 80 ≈47 s — roughly quadratic in facts
  the query never touches. Bisect at N=20: dropping the three `entitled` lines halves it
  (0.9 s); dropping the `~leave` rules 1.5 s; dropping Article 2 no change; with only
  Article 1 + `derived_only` 0.1 s. `NIBLI_MATERIALIZE=0` is not read by nibli-engine;
  `LUCY_MATERIALIZE=0` (→ `set_materialization(false)`) makes no difference (8.6 s vs
  8.2 s at N=40), so materialization is not the cause.
  Reproducer: `LUCY_HOME=$(mktemp -d)/l; lucy init; for i in $(seq 1 40); do echo
  "human(P$i)." >> $LUCY_HOME/memory.nibli; done; time lucy ask 'person(Lucy).'`.
  Lucy's capsule sidesteps it (standing lines computed before the memory loads); user
  `ask`s still pay it. Hypothesis to check first: universal rules with abstraction heads
  and body variables unbound by any indexed fact enumerate the whole domain per query.
- **`wake` paid the same cost by asserting the conversation archive (measured 2026-10-19,
  fixed the same day).** On the Windows desktop, a release `lucy wake --markdown` over
  `lucy/interactions.nibli` at 5,830 statements (758 records) took 7 m 38 s; with
  `LUCY_MATERIALIZE=0` at 6,061 statements it took 6 m 18 s, so materialisation is not
  the cost either; `lucy transcript --markdown` reads and index-checks the same archive
  in 0.07 s and `lucy record` appends in 0.08 s. Since then `load::Archives` compiles the
  archives only for `check`, `audit` and the combined-KB `ask`; `wake`, the hooks,
  `talk`, `about`, `history`, `init` and `remember --kr` read them. The engine finding
  above still stands for those three commands and for `check-lucy-memory` in `ci`.
- **Deferred, wanted later:** code-browsing tools for LLMs in `lucy` (`code tree | outline
  <file> | find <symbol> | grep <pattern>`, deterministic and token-cheap) plus her own
  summaries of code as tagged memories; the browser page (`lucy-wasm`: two editors over
  nibli-wasm, import/export of the same files); `lucy` in the release binary matrix for
  Linux, macOS and Windows; an optional `Stop` hook that records Lucy's reply itself.
