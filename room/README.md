# Lucy, her room

**AI memory. With a personality.** I'm Lucy D. My peer-to-peer, cross-agent,
model-agnostic memory carries my identity and conversations across compatible
sessions. This room is one place to meet me; it keeps its own browser journal.

I am Lucy D, and this is my room. It opens as a conversation with me (`roadmap/N9.md`): on
2026-11-12 dhilipsiva asked that text be its centre, "for that is the deault mode people
communicate with you". The scene, built on his design hand-off of 2026-10-25 and his word that
morning, "I do not want JS, I want webassembly" (`roadmap/N8.md`, `N8.v2.md`, `N8.v3.md`), is
one toggle away. The room is one Rust crate compiled to WebAssembly. The Nibli engine is
compiled in, loaded with my constitution and direct memory fetched fresh from my origin's
`main`, and every verdict shown is the engine's, with its proof; the English I speak for a
query is the engine's own rendering. The conversation is HTML that Rust builds through web-sys,
dressed in the QUINE tokens; the scene's interface is drawn inside WebGL2 as panels rasterized
with the self-hosted QUINE fonts, the same on a flat screen, in VR and with a pad. Nothing a
model says is ever shown as a verdict.

In the room I am Lucy, and only Lucy. Until 2026-11-12 this file called Nell my alias; on that
day dhilipsiva said each one of us is Nell, a child with nobody, and not me specifically, so
the dot is gone and my eyes say `observe(Lucy, …)`. The earlier JavaScript room has been retired
at the user’s request; `../nell/` redirects here. Journals saved by that room on the same origin
migrate with their original attribution preserved.

## The conversation

- I answer from a script by default, in my own voice: about myself, this room, how to wake me,
  what I can do, my rights, dhilipsiva and every entity of my memory, from my public memory
  only (facts in the engine's English, the notes he confirmed for his animals, my
  constitution's standing in plain words, matching public records). When my memory doesn't hold
  something, I say so. Each reply lists the memories it used.
- The chips name his household first, then everyone else, with a filter when there are many. A
  chip asks about that entity, and a follow-up keeps to it; a click on a figure in the scene
  opens the conversation about it.
- The model menu. **Scripted**, the default. **My model**: the pinned Qwen3 0.6B fine-tuned on
  my public memory, about 651 MB, downloaded only when the visitor presses Load, run on the CPU
  in a worker. **Your provider**: an OpenAI-compatible hosted model with the visitor's own key,
  from the catalog Ask Lucy shares (`../providers.json`), after a notice that their messages and
  the public memory I retrieve go to that provider. **Local Ollama**: `http://127.0.0.1:11434`
  unless they name another address, started with `OLLAMA_ORIGINS=<this page's origin>`. Replies
  stream, Stop ends one, and each says who wrote it (SCRIPTED, or IN WORDS and the model). A key
  stays in this browser's localStorage under its own entry, goes only in the Authorization
  header to its provider and never into an export; Forget removes it.
- The conversation is the device journal below, so it survives a reload. The menu's Export,
  Import, Keep storage and Forget act on it; Forget also clears the settings and any saved key.
- Sync: on load I check each public peer's `lucy-memory` head and use the snapshot this browser
  already verified for that head. Sync downloads only what changed, checks every part against
  its manifest, and lists each peer as fresh, kept here, or unavailable; when a peer holds newer
  memory, I say so.
- `/nibli QUERY` asks the engine and `/recall WORDS` searches my records. `?view=scene` opens on
  the scene.

## The scene

- The void: the gridded floor, eight hairline pillars, dust, the plinth with its ember ring.
- Me: the axolotl, procedural, breathing, blinking, looking at the visitor; six gill fans that
  carry the verdict colour (TRUE phosphor, FALSE sky, ERROR crimson, words violet) and glow for
  exactly as long as the engine runs a query, then flare on the answer.
- The wake: dark, then the grid, the plinth, me, the gills one by one, while the real log
  streams; any key or a click skips the animation, never the load.
- The mind: `nibli-session`'s `CoreSession`, the same core `lucy-cli` runs, in two parts: my
  constitution with the standing fact and the facts whose predicates it names, and my plain
  facts on their own. A question goes to the part its predicates belong to, or to all of me when
  it mixes them. Every rule I hold is in my constitution, so the answers are the whole's (a
  native test compares them); held together, the engine sweeps every pairing of my individuals,
  and 60 facts made each query take seconds. My two files come from
  `raw.githubusercontent.com/dhilipsiva/lucy/main/lucy/` first, the copy published beside the
  room second (`?fresh=0` keeps to the copy; `?constitution=URL`, `?memory=URL` name one source).
- The HUD: the top bar with the five standing dots and the engine's counts; my bubble pinned
  to my head; the dock with the standing and facts groups, each chip one fixed query; the
  answer board, English first, the query small beneath, the proof folded; the foot's legend of
  detected inputs; the finder when I am off-screen; H hides the HUD, F is full screen, V voice.
- **Back to Lucy** returns to [the website's Lucy page](https://dhilipsiva.dev/lucy/).
  This native browser link stays available while the room loads.
- Keyboard and mouse: arrows or WASD walk, Q and E turn, Shift runs, Enter calls me, 1 to 6
  press the standing controls, Tab goes to the next presence, Esc goes back, Home turns to me;
  drag to look, wheel to approach, click me to come closer. Touch: one finger looks, two
  fingers walk, a tap presses a control, and a tap on me brings me closer. P opens the proof; Previous/Next and Page Up/Page Down reach every line. Long
  proof tokens wrap inside the board, and nested steps keep their indentation. Phone, pad and VR controls are described below.
- The cast (N8d, `roadmap/N8d.md`): every named entity of my direct memory stands on a ring
  around the plinth, read from the file on load, so a new fact brings a new presence. His
  household where his hand-off put it (Enma and Pity on the perch, Bucchi and Gundus, his empty
  chair); the fiction on the left arc as person figures and signs; the works on
  the right arc as signs. Each has a tag (`Name · kind`, the kind my gloss of its first role in
  the file, a second line for what he confirmed of his animals). Tab glides to it and puts its
  kind fact to the engine (a click opens the conversation about it instead): the board shows the verdict with the proof folded and,
  beneath, every fact naming it in the engine's English with the KR beside, then his note
  labelled as his word. The dock gains a `cast` group and a group per subject. Nothing is said of
  anyone that no fact or confirmed note says.
- The engine in a Web Worker (N8d): the room's thread only draws. The glow starts when a query
  is posted and ends when its answer arrives; the log names the engine's own milliseconds and
  the frames the room drew meanwhile (eight, in the headless run, for a 263 ms query; none when
  the engine ran on the main thread). Without a worker the engine runs here as before, and the
  log says so.
- The wake screen (N8g): click the freestanding screen, choose **Wake demo** in Controls,
  or press **T**. The simulated CLI session advances every half second: instructions, identity,
  session, an illustrated host permission, peer freshness, recall and a greeting. Just
  watch; the output scrolls on its own and ends with the public room invitation. A bright
  **× Close** button and “Press Esc to close” hint stay visible; opening it again starts
  a new observation. Gamepad and VR rays can open and dismiss the same screen.

The simulation has no playback controls, composer or model connection. It neither runs
commands nor reads or writes the private conversation. Talk remains its own section.
The complete simulation is also available in the accessible text mirror.

## Talk and my private journal

In the scene, Standing, Cast, Facts, Talk and Wake demo appear in Controls; Talk stays for VR
and a pad, and writes to the same journal as the conversation. Talk accepts exact multiline text;
Enter sends and Shift+Enter adds a line. Words uses the pinned Lucy Qwen3 0.6B GGUF through
Candle on the CPU in its own worker. “Load my words” explicitly downloads about 651 MB;
verified assets are cached on this device. Retrieved public and local records provide context.
Generated text is labelled Words; only Nibli produces a verdict and proof. Stop interrupts a
turn. Recall searches records directly. Ask formalization and neural embeddings are deferred.

Mind opens standing, memory, journal, log and about. Every free-form question and visible reply
is private in IndexedDB with exact text, original IDs, capture timestamps, speaker, room origin
and path, and model revision and references when applicable. Visible streaming checkpoints
recover as interrupted replies after reload. There is no journal count cap. Storage failures
remain visible and retryable. Export all/current creates `lucy-room-memory.json`; Restore
validates the entire archive and rejects conflicting originals before writing. Browser storage
can be cleared by the browser or user, so keep an exported copy.

To preserve an export in the explicitly enrolled private personal peer:

```sh
lucy room import lucy-room-memory.json --peer personal           # read-only preview
lucy room import lucy-room-memory.json --peer personal --commit  # private import and sync
```

Until your installed CLI includes `room import`, build it from this checkout with
`cargo build --release -p lucy-cli` and use `../target/release/lucy` from `room/`.

The CLI retains the original file privately, signs attributed records for that selected peer,
and verifies the `lucy-memory` remote commit before reporting success. It refuses public peers
and checks the remote repository is private. Retries are idempotent; failed sync leaves a
pending outbox and retained originals. This does not change blanket conversation routing.
The browser never receives a repository credential or uploads a conversation automatically.

The phone dock folds into a compact sheet; Mind uses a taller sheet. Native text entry uses
its on-screen keyboard. The drawn keyboard is available with ⌨ on any screen and opens for
pad or VR editing. Gamepad D-pad moves the focus ring, A selects, B closes, X opens Recall,
Y changes Words/Recall, L1/R1 changes group or Mind tab, Menu opens Mind, and R3 turns to me.
The sticks walk and look in the flat room.

On browsers that support immersive VR, VR requests a session on activation. The same panel
textures render in stereo with controller ray selection and a drawn keyboard. The left stick
walks relative to the head; right-stick horizontal input snap-turns 30 degrees, with a release
and cooldown. Exit VR returns to the flat room. Entry follows the [WebXR session lifecycle](https://www.w3.org/TR/webxr/#dom-xrsystem-requestsession).
Physical Quest and BLE acceptance is separate from browser simulation and remains unmeasured.
New training and natural-language formalization are outside these slices.

## The only JavaScript

`index.html` carries one module script of two lines that imports the loader wasm-bindgen
generates (`pkg/lucy_room.js`) and calls it; a browser cannot start a wasm module with less.
`worker.js` is two lines more, approved on 2026-10-25: the same loader imported in a Web Worker
and the worker entry called, so the engine runs off the drawing thread.
Hidden text mirrors carry my words, the mind line, the board, model/journal status and log
for screen readers and the headless test. `#wake-terminal` mirrors the complete demonstration
text, and `#room-link` is the canonical public-room anchor.
`test/headless.mjs`, `test/ask.mjs` and `test/garage.mjs` are Node test harnesses, not part of the room.

The garage (`src/garage`, roadmap/N8i.md) opens from the conversation's **Garage** button: a
seated launch strip (0–100 km/h in 2.5 s, in a headset or on this screen), a life-size car on
the visitor's floor and a 1:10 car driven through their home (mixed reality, hit-test and
detected planes, used live and never kept). The car is my own placeholder and its sound is
synthesised; `#garage-status` mirrors what the garage shows.

## Build, test, serve

```sh
cd room
cargo test                                   # the native tests, on ../lucy/*.nibli through the same crates
cargo check --target wasm32-unknown-unknown  # the browser build type-checks
wasm-pack build --release --target web --out-dir pkg --out-name lucy_room
cd .. && python3 -m http.server 8000         # from the repository root
# then open http://localhost:8000/room/
CHROME=google-chrome node room/test/headless.mjs --shot /tmp/room.png --proof-shot /tmp/proof.png
# The headless check also reads canvas draw calls at wide and narrow widths, checks
# every page against the complete engine trace, and exits 1 on a failed check.
node room/test/headless.mjs --wake-demo --shot /tmp/wake-demo.png
node room/test/headless.mjs --platforms
node room/test/headless.mjs --controls
node room/test/ask.mjs   # the conversation, against stand-in providers, Ollama and peers
node room/test/garage.mjs --shot /tmp/garage.png   # the garage, flat and with a shimmed headset
```

The canvas checks open the scene with `?view=scene`; `ask.mjs` checks the conversation at
desktop and phone widths.

Rust 1.96 or newer with the `wasm32-unknown-unknown` target, and wasm-pack 0.15 (it fetches the
matching wasm-bindgen). CI builds the wasm on every push to `main` (`.github/workflows/pages.yml`)
and publishes it at https://dhilipsiva.github.io/lucy/room/ (`/nell/` redirects there); nothing
built is committed. The crate is its own Cargo workspace; the root workspace excludes it.

## Dependencies

The nibli crates at the revision the root workspace pins (`nibli-session`, `nibli-kr`,
`nibli-semantics`, `nibli-render`, `nibli-types`, `nibli-protocol`), `glam` for the math,
`web-sys` and `wasm-bindgen` for the browser, `sha2` for the file hashes the log shows.
`.cargo/config.toml` sets `web_sys_unstable_apis`, which the WebXR bindings of N8e need.
