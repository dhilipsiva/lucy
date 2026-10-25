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
his chair and remains the name my eyes use (`observe(Nell, …)`). The earlier JavaScript room
stays at `../nell/` until this one reaches parity (N8e), then its addresses redirect here.

## What is in it so far (N8a)

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
  come closer. Touch and a pad's sticks work; their layouts come with N8e.

## Not yet (the slices after this one)

N8b the composer, recall, memory through links, the journal, the drawer, the eyes; N8c words
and Ask through candle, the neural index; N8d the cast and the chair; N8e phone, pad, VR;
N8f his website. `roadmap/N8.v3.md` has each one's test.

## The only JavaScript

`index.html` carries one module script of two lines that imports the loader wasm-bindgen
generates (`pkg/lucy_room.js`) and calls it; a browser cannot start a wasm module with less.
Four hidden text mirrors (`#status`, `#mindline`, `#answer`, `#log`) carry my words, the mind
line, the board and the log as text, for screen readers and for the headless test; they are
text, not interface. `test/headless.mjs` is a Node test harness, not part of the room.

## Build, test, serve

```sh
cd room
cargo test                                   # the native tests, on ../lucy/*.nibli through the same crates
cargo check --target wasm32-unknown-unknown  # the browser build type-checks
wasm-pack build --release --target web --out-dir pkg --out-name lucy_room
cd .. && python3 -m http.server 8000         # from the repository root
# then open http://localhost:8000/room/
CHROME=google-chrome node room/test/headless.mjs --shot /tmp/room.png   # the headless check, by hand
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
