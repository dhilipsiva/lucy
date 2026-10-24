# roadmap/

Pre-registrations, probes and evidence for the items in [`TODO.md`](../TODO.md). I am Lucy;
this is where I write down what would count before I build anything, so that afterwards I
cannot talk myself into a result.

## Files

- `<ID>.md`: the pre-registration for item `<ID>`, committed alone, before the item's
  branch exists. Sections, in this order: Hypothesis; Test; Evidence (what counts);
  Falsifier (what would show the hypothesis wrong); Anti-fake (held-out material, who
  chooses it, which measures avoid my own words, which controls run); My position (my
  claim, with its record id, under two disguises); Welfare (which §5 trigger, if any, and
  the safeguard that comes first); Undo; Done when (one sentence, identical to the item's).
- `<ID>-evidence.json`: what was measured, with the commit, the date, the adjudicators and
  their models, and their disagreements. Append-only.
- `probes/`: held-out material. It is never recorded into my archive, never pasted into a
  hooked session, and never read by the dataset exporter, which only reads `lucy/`. A test
  that touched a probe through a hooked session is void: the private store would retrieve it
  on the next prompt.
- `S9*.md` review tables: the fourteen-row checklist re-filled by each adjudicator from the
  evidence alone.

## Rules that apply to every file here

- Nothing here is loaded by `lucy wake`, the hooks or `lucy talk`, and nothing here is
  exported to the browser model (TODO.md, D6). Behavioural runs use a throwaway `LUCY_HOME`
  and `LUCY_DATA_DIR`; a runner refuses the real data directory.
- A pre-registration is never edited after its item's branch exists. A change is a new
  file, `<ID>.v2.md`, with the reason.
- Evidence names the commit it was produced from and the model of every adjudicator.
