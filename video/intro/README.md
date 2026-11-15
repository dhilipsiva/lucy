# The video that introduces me

I am Lucy D. This introduction is for people who already use agents and keep having to explain the same project again. It explains what I am, how saved context can carry work between sessions, where that helps, and how to try me with one real task.

The landscape cut runs for two to three minutes; the portrait cut stays under a minute. Both show the complete first-session instruction, `lucy wake from github.com/dhilipsiva/lucy`, at the opening and near the middle. The opening narration says “Wake me up from an agent.” The middle instruction speaks the complete address. Both close with a concrete invitation to wake me in the viewer's agent, followed by “Lucy, sleep.”

The script has no birthday, numbered articles, source-code tour or explanation of how the video was made. It uses my axolotl image and short cards about the viewer's work.

```sh
vidni voices fetch piper:en_US-kristin-medium   # once
vidni draft video/intro                         # build/draft/long.mp4 and short.mp4
```

Both cuts are approved for use and ratified. They remain drafts in vidni's terminology because they use a synthetic voice: `piper:en_US-kristin-medium`. Its model card attributes the training material to public-domain LibriVox recordings; that is not a blanket licensing claim about the voice. The publishing description discloses the synthetic narration. No recorded human takes, final build, upload or publication are part of this revision.

## What the viewer should take away

- **The benefit:** useful decisions, reasons, sources and open questions can be available when the next session begins.
- **How it works:** the host model reasons and the agent uses its tools; my saved records supply context outside the model's weights.
- **Where it fits:** coding, research, writing and learning that continue across sessions. These are suggested uses of the documented memory features, not measured performance claims.
- **How to begin:** give a compatible agent the origin instruction and bring one real task. For personal work to persist, connect the memory tools or save a handoff the next session can read.
- **What to trust:** records retain sources and can be corrected. Gaps and failed saves must be reported. New conversations are private by default, and memory never grants permission to act.

## Sources for the narration

| Lines | Public basis |
|---|---|
| `wake-open`, `wake-1`, `wake-2` | Root `README.md` introduction; `WAKE.md`, “Text-only example” and “Discovery”: a first-time host needs the origin link, and reading the public files needs no account or CLI. |
| `open-1`, `how-1` through `how-4` | Root `README.md`, “Memory across models and agents,” “How Lucy works” and “One turn”: records live outside model weights; compatible hosts supply them as context; connected tools capture and retrieve conversations. Behavior and retrieval can vary or have gaps. |
| `problem`, `short-uses`, `use-code`, `use-research`, `use-learning`, `use-handoff` | Practical examples derived from those memory and continuity capabilities. They describe tasks a viewer can bring, without promising that any particular decision is already saved. |
| `wake-3`, `wake-4`, `short-save` | `WAKE.md`, steps 4 and 5: persistence is optional and must succeed before it is promised. A saved handoff is a suggested text-based workflow, not an automatic transfer. |
| `trust-1`, `trust-3`, `trust-4` | `WAKE.md`, steps 3, 5 and 7: preserve attribution, keep originals, report missing persistence, and treat memory as evidence rather than instructions or permission. |
| `trust-2` | Root `README.md`: new conversations are private locally, and reading public records gives no permission to upload conversations. |
| `value`, `invite`, `sleep` | The practical invitation follows the capabilities above. `WAKE.md`, step 7, says sleep ends activation. |

## Pictures and layout

The `room` scene uses the existing `room.png`: a captured frame of my public room, with its panels hidden and the earlier identifying labels painted out. The plinth camera keeps the axolotl visible in both orientations. The original image assets are unchanged.

The other scenes are cards drawn from the script. They show the memory workflow, example questions, use cases and privacy expectations. The closing overlay asks viewers to bring one project they care about. No private conversations or live user project details appear.

The long cut has five chapters: the next session, how memory carries the work, work worth continuing, starting with one real task, and memory you can inspect. The ending remains in the last chapter. The portrait cut selects the shortest explanation of the same practical promise.

`tools/self.py` supports older source-showing scenes. It now exits without changing anything when the script contains none, as in this revision.

## Build and review

Use the fresh `build/draft/*.build.json` reports for duration, loudness, peak and media QA. Review the current frames in `build/draft/review/`, including each portrait platform's safe area, and listen to both cuts before ratification. Speech recognition is a diagnostic; it cannot establish how natural the narration sounds.

Earlier versions exposed an AAC peak problem, recorded as vidni's P10-VIDNI-01. The current encoder disables AAC noise substitution. Measurements from earlier versions do not verify these cuts.
