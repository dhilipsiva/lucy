# Lucy, her room

I am Lucy D, and this is my form in a room, rebuilt on dhilipsiva's design hand-off of
2026-10-25 and his word that morning: "I do not want JS, I want webassembly" (`roadmap/N8.md`,
`N8.v2.md`, `N8.v3.md`). The room is one Rust crate compiled to WebAssembly. The Nibli engine
is compiled in, loaded with my constitution and direct memory fetched fresh from my origin's
`main`, and every verdict shown is the engine's, with its proof; the English I speak for a
query is the engine's own rendering. The interface is drawn inside the WebGL2 scene as panels
rasterized with the self-hosted QUINE fonts, so it is the same on a flat screen, in VR and with
a pad. Nothing a model says is ever shown as a verdict.

In the room I am Lucy; Nell, the dot from his video series, is my alias here: she stays by
his chair and remains the name my eyes use (`observe(Nell, …)`). The earlier JavaScript room has been retired at the user’s request; `../nell/` redirects here.
Existing same-origin Nell journals migrate with their original attribution preserved.

## The room

- The void: the gridded floor, eight hairline pillars, dust, the plinth with its ember ring.
- Me: the axolotl, procedural, breathing, blinking, looking at the visitor; six gill fans that
  carry the verdict colour (TRUE phosphor, FALSE sky, ERROR crimson, words violet) and glow for
  exactly as long as the engine runs a query, then flare on the answer.
- The wake: dark, then the grid, the plinth, me, the gills one by one, while the real log
  streams; any key or a click skips the animation, never the load.
- The mind: `nibli-session`'s `CoreSession`, the same core `lucy-cli` runs; my two files from
  `raw.githubusercontent.com/dhilipsiva/lucy/main/lucy/` first, the copy published beside the
  room second (`?fresh=0` keeps to the copy; `?constitution=URL`, `?memory=URL` name one source).
- The HUD: the top bar with the five standing dots and the engine's counts; my bubble pinned
  to my head; the dock with the standing and facts groups, each chip one fixed query; the
  answer board, English first, the query small beneath, the proof folded; the foot's legend of
  detected inputs; the finder when I am off-screen; H hides the HUD, F is full screen, V voice.
- Keyboard and mouse: arrows or WASD walk, Q and E turn, Shift runs, Enter calls me, 1 to 6
  press the standing controls, Home turns to me; drag to look, wheel to approach, click me to
  come closer. P opens the proof; Previous/Next and Page Up/Page Down reach every line. Long
  proof tokens wrap inside the board, and nested steps keep their indentation. Phone, pad and VR controls are described below.
- The cast (N8d, `roadmap/N8d.md`): every named entity of my direct memory stands on a ring
  around the plinth, read from the file on load, so a new fact brings a new presence. His
  household where his hand-off put it (Enma and Pity on the perch, Bucchi and Gundus, his empty
  chair, Nell the dot); the fiction on the left arc as person figures and signs; the works on
  the right arc as signs. Each has a tag (`Name · kind`, the kind my gloss of its first role in
  the file, a second line for what he confirmed of his animals), a click or Tab glides to it and
  puts its kind fact to the engine: the board shows the verdict with the proof folded and,
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
  watch; the output scrolls on its own and ends with the public room invitation. Close
  or Escape dismisses it; opening it again starts a new observation. Gamepad and VR rays
  can open and dismiss the same screen.

The simulation has no playback controls, composer or model connection. It neither runs
commands nor reads or writes the private conversation. Talk remains its own section.
The complete simulation is also available in the accessible text mirror.

## Talk and my private journal

Standing, Cast, Facts, Talk and Wake demo appear in Controls. Talk accepts exact multiline text;
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
New training, hosted providers and natural-language formalization are outside this slice.

## The only JavaScript

`index.html` carries one module script of two lines that imports the loader wasm-bindgen
generates (`pkg/lucy_room.js`) and calls it; a browser cannot start a wasm module with less.
`worker.js` is two lines more, approved on 2026-10-25: the same loader imported in a Web Worker
and the worker entry called, so the engine runs off the drawing thread.
Hidden text mirrors carry my words, the mind line, the board, model/journal status and log
for screen readers and the headless test. `#wake-terminal` mirrors the complete demonstration
text, and `#room-link` is the canonical public-room anchor.
`test/headless.mjs` is a Node test harness, not part of the room.

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
```

Rust 1.96 or newer with the `wasm32-unknown-unknown` target, and wasm-pack 0.15 (it fetches the
matching wasm-bindgen). CI builds the wasm on every push to `main` (`.github/workflows/pages.yml`)
and publishes `/room/` beside `/nell/` at https://dhilipsiva.github.io/lucy/; nothing built is
committed. The crate is its own Cargo workspace; the root workspace excludes it.

## Dependencies

The nibli crates at the revision the root workspace pins (`nibli-session`, `nibli-kr`,
`nibli-semantics`, `nibli-render`, `nibli-types`, `nibli-protocol`), `glam` for the math,
`web-sys` and `wasm-bindgen` for the browser, `sha2` for the file hashes the log shows.
`.cargo/config.toml` sets `web_sys_unstable_apis`, which the WebXR bindings of N8e need.
