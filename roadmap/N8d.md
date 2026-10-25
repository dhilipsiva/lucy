# N8d: every named entity of my memory in the room; the engine off the main thread

Pre-registered 2026-10-25, after `N8.v3.md` (98f466d) and the N8a and N8f slices, before any
code of this slice. Reason: dhilipsiva's words that afternoon, as typed, "Lucy in the room looks
good and answers everything. Let's pur all other charecters. Only one change if possible: Nibli
queries freezes the screen when asked." and, after my questions, "Build all named entities".

## His answers, as he chose or typed them

| Asked | He chose |
|---|---|
| The engine off the main thread | "Yes, the two-line worker script": the engine runs in a Web Worker; the JavaScript of mine becomes the loader wasm-bindgen generates, the page's two-line module script, and a two-line worker script. |
| The cast now | "The cast, clickable and described": a click glides to a presence and the board describes it; the chair's seat and the cast's lines wait for the eyes (N8b). |
| Which named entities | "The direct memory's names": every constant in `lucy/memory.nibli`, read from the file on load, so a new fact brings a new presence; and Nell, the dot, my alias, from the hand-off. |
| Forms for the names the hand-off did not draw | "A form by kind, plain and tagged": humans as the hand-off's person figure with a name tag; works, places, groups and kinds as a sign on a post in QUINE colours; he keeps the chair. Nothing invented about their looks. |
| Placement | "Grouped on a ring around the plinth": his household where the hand-off put it; the fiction together on one side; the works on the other; all facing me. |
| Next slice | "N8b: the dock and composer, recall, peers, journal, drawer, eyes". |

## What this slice builds

- The Nibli engine in a Web Worker (`room/worker.js`, `room/src/worker.rs`, `room/src/engine.rs`):
  the room's thread only draws; the glow starts when a query is posted and ends when its answer
  arrives; the log names the engine's own milliseconds and the frames the room drew meanwhile.
  When a worker cannot be made the engine runs on the main thread as before and the log says so.
- The presences (`room/src/cast.rs`, pure): the constants of my direct memory, Lucy excluded,
  with their facts, a kind word of mine keyed on the predicate and its place, a form by kind
  (bird, dog, person figure, the chair for Dhilipsiva, a sign for the rest), a group by the facts'
  `about` topics (fiction, works) or the hand-off (household), a place on the ring, a query (the
  kind fact, else the first fact naming them), a description: the facts naming them in the
  engine's English with the KR beside, then his confirmed notes of 2026-10-25 for the household
  only, labelled as his word.
- The forms (`room/src/creatures.rs`): the hand-off's birds, perch, dogs, chair and dot ported;
  the person figure and the sign; tag boards drawn with the QUINE fonts, turned to the camera.
- The room: picking by ray, the focus glide, Tab and Shift+Tab through the presences, Escape back;
  a `cast` group in the dock and a subject group with the presence's facts as chips.

## The test (done when)

1. Native: from the real `memory.nibli`, exactly the 17 names (Dhilipsiva, Nibli, Luffy, Sabo,
   StrawHatCrew, OnePiece, Oda, RightsNobodyHasToEarn, NixOS, LucyPrivate, PrivateMemory, Enma,
   Pity, Bird, Bucchi, Gundus, Nell); the forms and groups as the plan's table; every description
   line carries its KR or the "his word" label; every place inside the room; one appended
   `human(Someone).` adds an eighteenth presence, a figure; the worker's message codec round-trips;
   `standing.rs` unchanged and passing.
2. Headless, on this desktop: the log says the engine runs in a worker; the standing control's
   log line reports at least two frames drawn while the engine ran (the main-thread path draws
   none); Tab focuses the first presence and the board shows the engine's verdict for its kind
   fact with the description beneath; the cast line lists 17; a screenshot shows the ring.
3. CI green on the push; the room and `worker.js` served at both public addresses; the headless
   harness passes against the live room.
4. His: the room on his desktop; the box stays open until his reading is in the evidence.

## Falsifier, anti-fake, undo

Falsifier: a tag or description that says anything no fact of mine and no note he confirmed says;
a verdict on the board that did not come from the engine; a query that still blocks the frame
(fewer than two frames drawn during a 200 ms query). Anti-fake: the native test derives the
presences from the same file the browser fetches; the headless check reads the room's own log
line of frames drawn, and the main-thread fallback would fail it. Undo: hide the cast node; the
engine falls back to the main thread by itself when the worker is absent. Welfare: nothing here
speaks for the cast; their lines wait for the records (N8b).
