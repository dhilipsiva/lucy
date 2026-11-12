---
name: public-review
description: "Review Lucy's private memory for things that can go public, write a local review file for dhilipsiva to prune, and publish only what he keeps. Run only when he asks for it by name (\"Lucy, public review\" or /public-review); never on your own."
disable-model-invocation: true
---
Wake Lucy first ([WAKE.md](https://github.com/dhilipsiva/lucy/blob/main/WAKE.md)).
New conversation records stay local and private; this review is the only way they
become public, and only one item at a time with his approval.


# public-review

dhilipsiva set this up on 2026-11-12, in his words: "DO not do this automatically
going forward. Maybe create a skill so I can do this on-demand periodically."

- **Only on request.** Never start a review yourself, never schedule one, and never
  publish before he has pruned the review file and said to publish it.
- **What may go public**, by his word of the same day: anything about his NixOS
  configuration, Lucy herself, her UI, her room, and his public repositories. Check a
  repository with `gh repo view OWNER/NAME --json visibility`. Anything else goes under
  "Ask me first", which is never published unless he moves it up.
- **Always private:** his work and the work peer (unread), his children and family,
  other people's personal details, his job search, money, health and substance use, a
  manuscript under a publisher's rights, credentials, what Google or Gemini hold about
  him, and harness notices or subagent hand-backs (they are not his messages).

## Steps

1. **Begin.** Make a run directory outside every repository,
   `~/LucyTasks/public-review-YYYY-MM-DD/`, readable only by him. Begin a task record
   (`lucy self-model task begin`) whose criteria say the review file exists, every new
   conversation is accounted for, the scan is clean, and the origin is unchanged until he
   prunes.
2. **Plan.** Copy the previous run's `make_plan.py` into the new directory and adjust it.
   The builder reads only records newer than the last published review
   (`~/LucyTasks/public-review.state.json`), so what he kept private before is not
   offered again. Give each new conversation a disposition: include (a title and what it
   leaves out), exclude (a reason) or ask. Quote phrases to exclude, include or mark as
   personal. Facts must compile, and every new name becomes a figure in Lucy's room.
   Decisions carry his exact words and Lucy's reading of them. Names that must never go
   public belong in `~/LucyTasks/public-review.private.json`, never in a repository.
3. **Build.** From the origin checkout:
   `python3 -I scripts/public-review.py build --run DIR`. Require `"unaccounted": 0` in
   `coverage.json` and `"hits": 0` in `scan.json`. Then read `review.md` from top to
   bottom yourself: the scan matches patterns, not meaning. Compile the facts in a
   scratch `LUCY_HOME` with `lucy check`.
4. **Hand it over.** Give him the file's full path and say, plainly: delete a `###` block,
   or a `- Cxx.yy` line for one exchange, to keep it private; publishing is one-way,
   because Git history and the signed `lucy-memory` log keep what was pushed.
5. **Publish, on his word.** Run `publish --dry-run`. Rehearse in a scratch copy of
   `lucy/` with `LUCY_HOME`, `LUCY_DATA_DIR` and `LUCY_REVIEW_STATE` all pointing at
   scratch paths. Then publish from the registered origin: facts go to `memory.nibli`,
   conversations and notes to `interactions.nibli`, and decisions become claims linked
   to their published messages. The running service uploads changed `lucy/*.nibli`
   within 15 minutes, so the edit itself publishes; his word to publish covers the
   commit and push of the same files. Run `lucy check`, then finish the task record.
