# N8b + N8e: Room Controls, Platform Layouts, and Meta Quest 3 WebXR Plan

Dated 2026-10-26, on `main` at `99c6823`, after `N8a`, `N8f`, and `N8d` landed. Written as a
standalone engineering plan per dhilipsiva's instruction (*"Do not implement it. Simply write the
plan to a markdown file"*), leaving `roadmap/N8b.md` and `roadmap/N8e.md` uncreated until each
slice's pre-registration commit immediately precedes its branch (`roadmap/README.md`, lines 9–14
and 29).

## Scope and boundaries

On 2026-10-26 dhilipsiva asked to update `/lucy/room/`'s controls across both remaining UI/control
slices and to diagnose and fix why `/lucy/room/` does not load or work properly on his Meta Quest 3:

1. **Slice `N8b` — Dock, composer, memory & senses controls:** dock groups and paged chips;
   composer (`Ask · Words · Recall · Nibli`) backed by a hidden `<input>` for system keyboards;
   public peer archives read through `peers.json`; exact + IDF-weighted sparse hashed vector recall
   persisted in `IndexedDB`; device-local journal in `localStorage` (`nell.journal.v1`) with JSON
   export and forget; 5-tab `Mind` drawer (`standing · memory · journal · log · about`); finder;
   `Eyes` (`3` hits `>= 0.6` in `3 s`, `5 s` vanish; people only `Person`) with simulator + Web
   Bluetooth Nordic UART; chair seat assumption/denial; KB-grounded household cast boards and my
   relay.
2. **Slice `N8e` — Platform controls & Meta Quest 3 VR:** root-cause fixes for Quest Browser 2D
   and immersive loading/interaction; phone bottom-sheet layout; gamepad/DualSense focus ring and
   button scheme; in-scene drawn virtual keyboard; full WebXR `immersive-vr` stereo loop, 3D panels
   standing in the room, Quest Touch controller rays, and thumbstick locomotion + snap-turn.

**Non-negotiable constraints (`roadmap/N8.v3.md`, `lucy.md`):**
- **All of it in Rust → WebAssembly (`room/`).** The only JavaScript in `room/` remains the
  wasm-bindgen loader, the two-line module boot in `room/index.html`, and the two-line module worker
  boot in `room/worker.js`.
- **Zero new Cargo dependencies.** `web-sys 0.3.106` is already locked in `room/Cargo.lock`, and
  `room/.cargo/config.toml` already passes `--cfg=web_sys_unstable_apis` for both host and
  `wasm32-unknown-unknown` builds. Only additional feature flags on `web-sys` in `room/Cargo.toml`
  are required, keeping `cargo test --locked` offline-reproducible.
- **`N8c` stays separate.** The 0.6B `candle` weights for local `Words` generation and local `Ask`
  formalization remain in `N8c`. Until `N8c` lands, `Nibli` and `Recall` execute completely; `Ask`
  parses valid Nibli directly if typed as KR and otherwise states plainly that the local formalizer
  arrives in `N8c` (offering one-click fallback to `Recall`); `Words` states plainly that the 0.6B
  weights arrive in `N8c`. No model ever writes an engine verdict.
- **Keep `nell/` live until verified.** `nell/` is not retired or redirected until dhilipsiva has
  tested `/lucy/room/` on both desktop and his Meta Quest 3.

---

## Part 1 — Root-Cause Diagnosis: Why `/lucy/room/` Fails on Meta Quest 3 Today

Inspection of `room/`, `nell/`, the QUINE hand-off, and both live deployments
(`https://dhilipsiva.dev/lucy/room/` and `https://dhilipsiva.github.io/lucy/room/`) identified six
concrete root causes explaining why `/lucy/room/` fails on Meta Quest 3:

### 1. WebXR is completely unimplemented in `room/`
- `Action::Xr` in `room/src/app.rs:868` is matched to an empty block `{}`.
- `Hud::draw_top` in `room/src/hud.rs:253-281` draws only `∿` (voice), `⤢` (fullscreen), and `H`
  (hide HUD) in `TOP_R` — there is no `"Enter in VR"` button and `navigator.xr` is never queried.
- `App` drives frames exclusively via `window.requestAnimationFrame` (`room/src/app.rs:261-268`),
  which Quest Browser throttles or pauses when entering immersive VR instead of handing off to
  `XrSession::request_animation_frame`.
- `Hud::render` (`room/src/hud.rs:852`) draws panels exclusively in 2D screen-space orthographic
  coordinates into WebGL's default framebuffer (`null`), which is invisible inside an `XRWebGLLayer`
  headset framebuffer.

### 2. Module Worker URL resolution & silent top-level `await` hang (`10 s` dark stall)
- `Engine::spawn` (`room/src/engine.rs:69`) calls
  `Worker::new_with_options("./worker.js", &opts)` *after* `await init()` inside `./pkg/lucy_room.js`.
  Depending on the browser's incumbent script base URL rules — or when `/lucy/room` is opened
  without a trailing slash — `./worker.js` can resolve against `/lucy/room/pkg/lucy_room.js`
  (`/lucy/room/pkg/worker.js` → `404`) or `/lucy/worker.js` (`404`).
- Worse, `room/worker.js:1-2` is:
  ```js
  import init, { worker_main } from './pkg/lucy_room.js';
  await init(); worker_main();
  ```
  Inside a module worker, an unhandled rejection from top-level `await init()` does **not** fire
  `worker.onerror` in Chromium/Quest Browser. `ready` stays `false`, trapping the room in dark wake
  for the full `10_000 ms` timeout (`room/src/engine.rs:163`) before falling back to the main
  thread.
- **Fix:** Resolve `worker.js` explicitly from `window.location`'s directory URL in Rust; keep
  `room/worker.js` two lines while routing initialization failures back as a `fault` message:
  ```js
  import init, { worker_main } from './pkg/lucy_room.js';
  init().then(worker_main, (e) => postMessage(JSON.stringify({ kind: 'fault', error: String(e) })));
  ```
  and tighten the silent-worker fallback timeout in `room/src/engine.rs:163` from `10_000.0` ms to
  `4_000.0` ms.

### 3. Eager `xrCompatible: true` on `getContext("webgl2")` without fallback
- `Gl::new` (`room/src/gl.rs:156, 163-168`) passes `xrCompatible: true` and
  `powerPreference: "high-performance"` unconditionally at page boot, with no fallback if
  `getContext` returns `None` or rejects the XR-compositor attachment prior to a user gesture.
- **Fix:** Omit `xrCompatible: true` at initial boot (retrying with default `{}` options if the
  first `get_context_with_context_options` returns `None`), and invoke
  `JsFuture::from(gl.ctx.make_xr_compatible()).await` inside the `"Enter in VR"` user-gesture task
  immediately before creating `XrWebGlLayer`.

### 4. `2.0 px` pointer-drag threshold swallows Quest Touch laser-ray clicks in 2D mode
- Even in Quest Browser's floating 2D window, pointing with a Quest Touch controller maps trigger
  press/release to `pointerdown` / `pointermove` / `pointerup`. Pulling the physical trigger causes
  natural wrist micro-movement of `3–12` CSS pixels.
- In `room/src/app.rs:959, 971`, `if dx.abs() + dy.abs() > 2.0 { d.2 = true; }` marks the gesture
  as `moved = true` on a single 2-pixel jitter, causing `PointerUp` (`room/src/app.rs:998`) to skip
  `self.click(x, y)`. Virtually every button, chip, and presence is unclickable with a Quest
  controller in 2D!
- **Fix:** Record `(start_x, start_y, last_x, last_y, started_on_hud, moved)` on `PointerDown`.
  Measure total displacement `hypot(x - start_x, y - start_y)` against `10.0` CSS px (`14.0` px for
  touch). When `started_on_hud` is true (`self.hud.over_panel(start_x, start_y)`), do **not** orbit
  the 3D camera during `PointerMove`, and still dispatch `self.click(x, y)` on `PointerUp` if release
  stays within `24.0` px of `start` or hits the same HUD `Action`.

### 5. WebXR side-by-side stereo `gl.clear()` hazard & invalid `domOverlay` option
- Meta Quest 3's `XRWebGLLayer` uses a single side-by-side framebuffer (`width = 2 * eyeWidth`, left
  eye `[0..w/2]`, right eye `[w/2..w]`). In WebGL2, `gl.clear()` ignores `gl.viewport()` unless
  `SCISSOR_TEST` is enabled. Calling `gl.clear()` inside the per-eye loop wipes the left eye to
  black when clearing for the right eye!
- Meanwhile, `nell/nell.js:414` passes `domOverlay: { root: hud }` inside `requestSession('immersive-vr', ...)`,
  which is a WebXR `immersive-ar` feature and causes `DOMOverlay` warnings or session-init quirks in
  `immersive-vr`.
- **Fix:** Bind `layer.framebuffer()` once per `XrFrame`, set full framebuffer viewport, call
  `gl.clear(col::VOID)` **once**, and then loop over `pose.views()` setting per-eye `gl.viewport`
  and rendering scene + 3D HUD quads + controller rays. Request `immersive-vr` with
  `optionalFeatures: ["local-floor", "hand-tracking"]` only (falling back from `local-floor` to
  `local` with a `+1.6 m` `EYE` height offset if needed).

### 6. Main-thread archive indexing stall & `MAX_TEXTURE_SIZE` bounds on mobile GPUs
- `lucy/interactions.nibli` currently has `9,876` lines (~`1.8 MB`). Synchronously chunking and
  hashing ~`10,000+` chunks on the main thread during load would freeze Quest 3's `72/90/120 Hz`
  compositor for multiple seconds.
- `Panel::resize` (`room/src/panel.rs:90-99`) allocates backing 2D canvases at `(w * dpr).ceil()`
  without clamping to `gl.get_parameter(MAX_TEXTURE_SIZE)`.
- **Fix:** Process peer/archive chunking and `HashSpace` embedding in yielded async slices of
  `~200` chunks per animation frame (logging progress in `Mind → memory`), and clamp panel texture
  dimensions in `Panel::resize` by scaling effective `dpr` so `canvas.width` and `canvas.height`
  never exceed `min(max_texture_size, 4096)`.

---

## Part 2 — Slice `N8b`: Dock, Composer, Recall, Peers, Journal, Mind Drawer, Eyes, Seat & Cast Lines

### 2.1 `room/Cargo.toml` feature additions for `N8b`
Add to `web-sys` features (all already present in locked `web-sys 0.3.106`):
- Local storage & file export: `"Storage"`, `"Blob"`, `"BlobPropertyBag"`, `"Url"`, `"HtmlAnchorElement"`
- Hidden composer input sync: `"FocusEvent"`, `"InputEvent"`
- IndexedDB vector persistence: `"IdbFactory"`, `"IdbOpenDbRequest"`, `"IdbVersionChangeEvent"`,
  `"IdbDatabase"`, `"IdbObjectStore"`, `"IdbTransaction"`, `"IdbTransactionMode"`, `"IdbRequest"`
- Web Bluetooth Nordic UART (`--cfg=web_sys_unstable_apis` already active): `"Bluetooth"`,
  `"BluetoothDevice"`, `"BluetoothRemoteGattServer"`, `"BluetoothRemoteGattService"`,
  `"BluetoothRemoteGattCharacteristic"`, `"RequestDeviceOptions"`, `"BluetoothlescanFilterInit"`

### 2.2 Public peer archives (`room/src/peers.rs`)
Port `nell/memory.js` into pure Rust + `sources.rs` fetches:
- **Registry discovery:** Check `?registry=<url>` first, then `../peers.json` (fresh raw GitHub URL
  when `?fresh != 0`, published relative URL as fallback), else synthesize a single default `origin`
  peer pointing to `../memory.nibli` and `../interactions.nibli`.
- **Record parser (`parse_records`):** Parse `record("<id>", "<ts>", <Kind>, "<escaped-json>").`
  lines (`Kind` in `Prompt | Note | Observation | Correction | Fact`). Unescape `\"`, `\\`, `\n`,
  `\r`, `\t`, parse inner JSON (`text`, `prompt`, `response`, `summary`, `channel`, `about`, `who`,
  `source`), deduplicate across peers by `id` (keeping first peer provenance), and sort
  chronologically by `(ts, order)`.
- **Exact recall & topic summary:**
  - `learned(&[Peer])`: loaded peer count, deduplicated record count, breakdown by kind, latest
    timestamp, and per-peer errors/counts.
  - `recall_exact(records, needle, limit)`: case-insensitive substring match over `hay`, returning
    most-recent matches first (`limit = 5`).
  - `top_topics(records, k)`: top `about` tags with frequencies for chips and `Mind → memory`.
  - `record_line(entry)`: produces valid `.nibli` `record(...)` syntax for journal export parity.

### 2.3 Sparse hashed vector space & `IndexedDB` cache (`room/src/recall.rs`)
Port `nell/vectors.js` cleanly into Rust with native unit tests matching `nell/test/vectors.mjs`:
- **Chunking & tokenization:**
  - `chunks(text, max_chars = 320)`: split on blank lines, pack paragraphs up to `320` chars, slice
    oversized paragraphs cleanly.
  - `tokenize(text)`: lowercase ASCII/Unicode alphanumeric runs of length `>= 2`.
  - `fnv1a_utf16(s)`: exact UTF-16 code-unit FNV-1a (`0x811c9dc5`, prime `0x01000193`) for
    deterministic cross-platform parity.
- **Features & `HashSpace` (`KIND = "hash-v2"`, `DIMS = 384`, `HASH_FLOOR = 0.12`):**
  - Unigrams (`w:{tok}`, weight `1.0`), adjacent token bigrams (`2:{a}_{b}`, weight `1.25`), and
    character trigrams on `^tok$` for tokens `>= 3` chars (`c:{tri}`, weight `0.35`).
  - Smoothed corpus IDF `ln((n + 1.0) / (df + 1.0)) + 1.0`, signed slot projection into `384`
    dimensions, and L2 normalization into unit `Vec<f32>`.
- **`IndexedDB` persistence (`nell-vectors`, version `1`, store `spaces`):**
  - Store `{ kind: "hash-v2", dims: 384, hashes: Vec<u32>, vectors: Float32Array, df: Vec<(u32, u32)>, docs: u32 }`
    under key `"public-hash-v2"`.
  - On wake, reuse cached vectors whose `fnv1a_utf16(chunk_text)` matches; embed only new/changed
    chunks in yielded batches of `200` chunks per frame so frame times stay smooth on desktop and
    Quest 3.
  - `search(query, k = 5)`: query `HashSpace`, drop hits `< 0.12`, merge best score per parent
    record (`best_chunk` snippet retained), and fall back to `recall_exact` if vector index is still
    building or scores below floor while substring matches exist.

### 2.4 Device-local journal (`room/src/journal.rs`)
Port `nell/journal.js` using identical storage keys so existing browser journals carry over:
- Keys: `nell.journal.v1` (JSON array, capped at `CAP = 2000` newest entries) and `nell.device.v1`
  (stable random hex device suffix).
- Entry schema: `{ id, ts, kind, payload: { text, summary, who, about, private: true }, line, local: true }`.
  Every device note is explicitly marked `private: true` and never leaves the browser unless the
  visitor clicks **Export JSON**.
- Operations: `load()`, `append(kind, text, who, about)`, `recent(limit)`, `recall_local(query)`
  (merged into `Recall` results with `[this device · private]` badge), `export_json_download()` via
  `Blob` + temporary `<a download="lucy-journal-<device>.json">`, and `forget_all()` (two-click
  confirmation before clearing `nell.journal.v1`).

### 2.5 Eyes, seat assumption, household cast boards & relay (`room/src/eyes.rs`, `worker.rs`, `world.rs`, `app.rs`)
Port `nell/senses.js` and wire live KB assertions into the worker engine:
- **Hysteresis & welfare privacy rule (`room/src/eyes.rs`):**
  - Constants: `NEED_HITS = 3`, `MIN_CONFIDENCE = 0.6`, `WINDOW_MS = 3_000.0`, `VANISH_MS = 5_000.0`.
  - Classes mapped to KB atoms: `"bird" -> Some("Bird")`, `"dog" -> Some("Dog")`,
    `"person" | "human" | "face" -> Some("Person")`, everything else ignored. People are **never**
    individually identified by camera/sensor frames — only `Person` (`roadmap/N8.md` welfare rule).
  - Three detections `>= 0.6` within `3 s` emit `SenseEvent::Arrived { atom: "Bird" | "Dog" | "Person" }`;
    `5 s` without a qualifying hit emits `SenseEvent::Left { atom }`.
- **Worker KB mutation (`room/src/worker.rs`, `room/src/engine.rs`):**
  - Add `ToWorker::See { atom: String, present: bool }` and
    `FromWorker::Seen { atom: String, present: bool, fact_count: usize }`.
  - Inside `worker::handle`, maintain `observed_fact_ids: BTreeMap<String, Vec<u64>>`:
    - When `present == true`: retract any prior IDs for `atom`, call
      `mind.assert_text(&format!("observe(Nell, {atom})."))`, store returned fact IDs, and reply
      `FromWorker::Seen`.
    - When `present == false`: call `mind.retract_fact(id)` for stored IDs and reply `FromWorker::Seen`.
- **Visual visitor forms (`room/src/world.rs`, `room/src/creatures.rs`):**
  - Reuse the sky-blue meshes already defined in `room/src/creatures.rs` (`UNKNOWN_BIRD`,
    `UNKNOWN_DOG`, and `Figure` tinted in `SKY_400`) placed near the front-right of the plinth while
    active, fading in on `Arrived` and out on `Left`, with a matching journal/log observation entry.
- **Simulator & Web Bluetooth Nordic UART:**
  - `eyes` dock group chips: `Bird arrives`, `Dog arrives`, `Person arrives`, `Clear eyes`, and
    `Connect BLE eye` (shown when `navigator.bluetooth` exists).
  - BLE UART (`6e400001-b5a3-f393-e0a9-e50e24dcca9e`, TX notify char `6e400003-b5a3-f393-e0a9-e50e24dcca9e`):
    buffer UTF-8 chunks across `\n`, parse JSON lines (`{ "class": "bird", "score": 0.88 }` or
    `{ "detections": [...] }`), and feed `Senses::ingest`.
- **Chair seat assumption & household cast lines + relay:**
  - Default `seat_assumed = true` (dhilipsiva's chair shows a warm presence indicator and board note
    *"Seat assumed: dhilipsiva (default). Say if not."* with chip `"I am not dhilipsiva"` / `"I am dhilipsiva"`).
    Toggling updates `who` on local journal entries (`"dhilipsiva"` vs `"visitor"`).
  - Clicking household presences (`Enma`, `Pity`, `Bucchi`, `Gundus`, `Nell`, `Dhilipsiva`) shows
    only their KB-derived facts + confirmed 2026-10-25 household notes (`cast.rs`), any recalled
    record mentioning their name from `peers.rs` (explicitly labelled `record`), and my **Relay**
    prompt when a visitor asks while focused on a cast member: chips `"Yes — put it to the engine"`,
    `"Yes — recall it"`, and `"Not now"`, so I never ventriloquize or invent ungrounded speech for
    them.

### 2.6 Dock groups, composer & `Mind` drawer (`room/src/hud.rs`, `room/src/panel.rs`, `room/index.html`)
- **Hidden `<input>` mirror in `room/index.html`:**
  - Add `<input id="composer-input" class="sr" type="text" autocomplete="off" autocapitalize="off" spellcheck="false" aria-label="Ask or write Nibli">`
    beside `#bubble-mirror` and `#log-mirror`.
  - Clicking the drawn composer box focuses `#composer-input` (triggering OS / Quest system keyboard
    when desired) and syncs `value` + selection caret into `App.draft`. When `#composer-input` is
    focused, single-letter HUD hotkeys (`h`, `f`, `v`, `p`, `1..6`, `wasd`) are bypassed so normal
    typing works uninterrupted; `Enter` submits and `Escape` blurs.
- **Full dock (`DOCK` panel):**
  - **Group tabs:** `[subject]` (when a cast presence is focused), `standing`, `facts`, `eyes`,
    `recall`, `words`, `cast`.
  - **Chip strip with horizontal paging:** When chips exceed panel width, draw `‹` and `›` buttons
    (`Action::ChipPage(-1 | 1)`) instead of clipping chips off-screen.
  - **Composer bar:** Mode pill selector (`Ask · Words · Recall · Nibli`), text field with blinking
    ember caret (`col::EMBER_500`), clear `×` button when non-empty, keyboard icon `⌨` to toggle the
    in-scene drawn keyboard, and `Send ↵` button.
    - `Nibli`: sends KR directly to the worker engine and displays verdict + proof in `BUBBLE`.
    - `Recall`: runs `HashSpace` + exact + local journal search and displays ranked record excerpts
      with provenance (`peer`, `id`, `ts`, score, or `private device note`) in `BUBBLE` (`Kind::Recalled`).
    - `Ask`: if `draft` parses cleanly as Nibli KR (ends with `?` or `.` and passes parser check),
      dispatches directly to the engine; otherwise displays a clear `Note` in `BUBBLE` explaining
      that local NL→Nibli formalization (`candle` 0.6B) lands in `N8c`, with a one-click chip to run
      the same question through `Recall` right now.
    - `Words`: displays a clear `Note` that local `0.6B` `candle` weights land in `N8c`, with a
      one-click chip to run `Recall` or `Note to journal`.
- **`Mind` button & 5-tab `Mind` drawer (`MIND` panel):**
  - Add `M` (`Mind`) button to `TOP_R` (and `M` keyboard shortcut when composer is unfocused).
  - Drawer tabs (`Action::MindTab(MindTab)`):
    1. `standing`: the six standing claims, their live verdicts, KR formulas, and one-click re-verify.
    2. `memory`: loaded files (`memory.nibli`, `interactions.nibli`), `peers.json` status, record
       counts by kind, top `about` topics (clickable to recall), and `HashSpace` / `IndexedDB` build
       progress (`cached` vs `indexing X / Y`).
    3. `journal`: device ID, `private: true` guarantee, quick `"Save current draft as note"` button,
       scrollable recent entries, `Export JSON`, and two-step `Forget device journal`.
    4. `log`: full scrollable timestamped session log (engine load/worker ms, frames drawn during
       queries, eye arrivals/departures, XR state).
    5. `about`: honest architecture summary (Rust → wasm, compiled-in Nibli engine in a Web Worker,
       what leaves the device: only public file fetches, never device notes).
- **Finder (`TOP_L` / `Action::Find`):** Clicking my name/dot in `TOP_L` or pressing `Home` smoothly
  glides focus back to me (`world.focus_lucy`).

---

## Part 3 — Slice `N8e`: Mobile Sheet, Gamepad Scheme, Drawn Keyboard, WebXR & Meta Quest 3 Parity

### 3.1 `room/Cargo.toml` feature additions for `N8e`
Add to `web-sys` features (all verified in `web-sys 0.3.106` behind the existing
`--cfg=web_sys_unstable_apis` flag):
- Framebuffer binding: `"WebGlFramebuffer"`
- WebXR core & stereo views: `"XrSystem"`, `"XrSession"`, `"XrSessionMode"`, `"XrSessionInit"`,
  `"XrFrame"`, `"XrReferenceSpace"`, `"XrReferenceSpaceType"`, `"XrViewerPose"`, `"XrView"`,
  `"XrRigidTransform"`, `"XrViewport"`, `"XrWebGlLayer"`, `"XrWebGlLayerInit"`, `"XrRenderState"`,
  `"XrRenderStateInit"`
- WebXR controllers & rays: `"XrInputSource"`, `"XrInputSourceArray"`, `"XrInputSourceEvent"`,
  `"XrHandedness"`, `"XrTargetRayMode"`, `"XrSpace"`, `"XrPose"`, `"DomPointReadOnly"`

### 3.2 Quest Browser 2D & core rendering hardening (`engine.rs`, `worker.js`, `gl.rs`, `panel.rs`, `app.rs`)
Apply all six fixes diagnosed in Part 1:
1. Resolve `worker.js` against `window.location` base directory URL; update `room/worker.js` line 2
   to catch `init()` rejections and post `FromWorker::Fault`; lower worker ready timeout to `4_000 ms`.
2. Remove eager `xrCompatible: true` from `Gl::new` (`room/src/gl.rs:156`), add fallback `{}` context
   creation if `high-performance` returns `None`, and add `Gl::bind_framebuffer(&self, fb: Option<&WebGlFramebuffer>)`.
3. Query `MAX_TEXTURE_SIZE` once in `Gl::new` and clamp `Panel::resize` (`room/src/panel.rs:90`) so
   canvas backing dimensions never exceed `min(max_tex_size, 4096)`.
4. Replace the `2.0 px` incremental drag check in `room/src/app.rs:937-1001` with start-anchored
   hysteresis (`10 px` mouse/controller ray, `14 px` touch) and HUD-panel pointer capture so Quest
   Browser 2D laser clicks register reliably on the first trigger pull.

### 3.3 Phone / narrow-screen bottom-sheet layout (`room/src/hud.rs`)
Active when `(st.touch && !st.pointer) || st.w < 760.0` (and `!st.xr_active`):
- **Compact top bar (`40 px` height, `8 px` margin):** `TOP_L` shows `● Lucy` + fact count badge;
  `TOP_R` uses `40×40 px` touch targets (`∿`, `M`, `H`).
- **Answer peek / expanded `BUBBLE`:** Anchored directly above the bottom sheet (`w - 16 px`);
  collapses to a `44 px` single-line verdict peek bar when the sheet or Mind drawer is expanded,
  tappable to expand full proof/answer.
- **Bottom-sheet `DOCK` (`x = 0`, `w = st.w`, flush bottom with rounded top corners `14 px`):**
  - Centered `36×4 px` grab handle at top (`Action::ToggleSheet` between compact peek and full dock).
  - `44 px` tall group pills and horizontally paged chips.
  - Two-row mobile composer: mode pills + `⌨` + `Send` on row 1, full-width input box on row 2 so
    thumb targets never crowd below `44 px`.
- **Sheet-mode `Mind` drawer:** Covers bottom `78%` of viewport height with `44 px` tab strip and
  large `×` close button.

### 3.4 Gamepad / DualSense focus ring & button scheme (`room/src/input.rs`, `room/src/hud.rs`, `room/src/app.rs`)
Active when `flags.gamepad && !xr_active`:
- **Focus navigation grid (`FocusZone`):**
  `Groups` ↔ `Chips` ↔ `Composer` (`Mode`, `Input`, `KeyboardToggle`, `Send`) ↔ `BubbleActions`
  (proof toggle / relay chips) ↔ `TopRight`, plus `MindDrawer` and `VirtualKeyboard` when open.
- **Visual treatment:** Dock widens to `min(1280.0, w - 48.0)` with `44 px` chip height; focused
  control receives a crisp `2 px` `EMBER_500` (`#e68a3e`) outline and subtle ember glow; unfocused
  panels stay clean.
- **Standard Gamepad / DualSense edge-triggered bindings (`room/src/app.rs:647`):**
  - D-pad Up/Down/Left/Right (`buttons[12..=15]`): move focus ring across/within rows (or 2D grid on
    the drawn keyboard).
  - Cross / `A` (`buttons[0]`): activate focused control / press focused virtual key.
  - Circle / `B` (`buttons[1]`): close `VirtualKeyboard` → close `Mind` drawer → clear focused
    subject.
  - Square / `X` (`buttons[2]`): switch composer mode to `Recall` and focus composer.
  - Triangle / `Y` (`buttons[3]`): switch composer mode to `Words` (or cycle mode when already in
    `Words`).
  - `L1` / `R1` (`buttons[4]`, `buttons[5]`): previous / next dock group (or `Mind` tab when drawer
    is open).
  - `Options` (`buttons[9]`): toggle `Mind` drawer.
  - Left stick (`axes[0..2]`): walk (`world.walk`); Right stick (`axes[2..4]`): look (`world.look`);
    `R3` (`buttons[11]`): `Find` (turn to Lucy).
- **`PadHints` footer strip:** Drawn along the bottom of `DOCK` (`✕ Select · ○ Back · □ Recall · △ Mode · L1/R1 Group · Opt Mind`).

### 3.5 In-scene drawn virtual keyboard (`KEYBOARD` panel in `room/src/hud.rs`)
- Add `KEYBOARD` (`usize = 6`) to `Hud.panels`.
- Opens automatically when editing composer via gamepad or inside WebXR `immersive-vr` (where HTML
  `<input>` overlays are not visible), or manually via the composer's `⌨` button on any platform.
- **Key layout (`5` rows, `40–44 px` key caps):**
  - Row 1: `1 2 3 4 5 6 7 8 9 0 ( )`
  - Row 2: `q w e r t y u i o p { }`
  - Row 3: `a s d f g h j k l : " '`
  - Row 4: `⇧ z x c v b n m , . ? ⌫`
  - Row 5: `Nibli [ $ _ ¬ ∧ ∨ → ]` · `Space` · `Clear` · `Hide` · `Send ↵`
- Every key emits `Action::VKey(VKey)` updating `App.draft` (and mirroring into `#composer-input`),
  clickable by mouse, touch, gamepad focus ring, and VR controller laser ray.

### 3.6 WebXR `immersive-vr` for Meta Quest 3 (`room/src/xr.rs`, `gl.rs`, `world.rs`, `hud.rs`, `app.rs`)
Create `room/src/xr.rs` encapsulating session lifecycle, stereo views, 3D panel placement, and Quest
Touch controller input:
1. **Capability probe & `"Enter in VR"` button:**
   - On `App` startup, check `window.navigator().xr()`. Call
     `xr.is_session_supported(XrSessionMode::ImmersiveVr)` asynchronously; when `true`, set
     `app.xr_supported = true` (`version += 1`).
   - `TOP_R` displays a prominent `"VR"` / `"Enter in VR"` pill (`Action::Xr`) whenever
     `xr_supported` is true (and `"Exit VR"` inside the VR dock).
2. **Entering VR (`Action::Xr` inside user gesture):**
   - Call `JsFuture::from(gl.ctx.make_xr_compatible()).await`.
   - Build `XrSessionInit` with `optional_features(["local-floor", "hand-tracking"])` — **no**
     `domOverlay`.
   - Request `xr.request_session_with_options(XrSessionMode::ImmersiveVr, &init)`.
   - Create `XrWebGlLayer::new_with_web_gl2_rendering_context_and_layer_init(&session, &gl.ctx, &layer_init)`
     and apply via `session.update_render_state_with_state(&render_state)`.
   - Request reference space `"local-floor"`; if rejected, fall back to `"local"` and set
     `standing_offset_y = 1.6` (`world::EYE`).
   - Attach `end` event listener restoring `app.xr_active = false`, re-binding default framebuffer
     `None`, resizing canvas to window dimensions, and resuming `window.requestAnimationFrame`.
   - Attach `selectstart` listener on `XrSession` to trigger ray clicks on 3D HUD panels or 3D
     presences.
3. **Stereo frame loop (`XrSession::request_animation_frame`):**
   - While `xr_active` is `true`, `window.requestAnimationFrame` returns early so the two loops
     never double-step simulation.
   - On each `XrFrame`:
     - Advance `dt`, pump worker engine, update `Senses`, poll `session.input_sources()` for
       thumbstick locomotion and laser rays, and sync HUD canvases (`hud.sync(&gl, &state)`).
     - Obtain `pose = frame.get_viewer_pose(&ref_space)`.
     - Bind `gl.bind_framebuffer(layer.framebuffer().as_ref())`.
     - Set full layer viewport `(0, 0, layer.framebuffer_width(), layer.framebuffer_height())` and
       call **`gl.clear(col::VOID)` once** before iterating views.
     - Maintain player rig transform `rig_mat = Mat4::from_rotation_translation(Quat::from_rotation_y(rig_yaw), rig_pos)`
       (initialized facing Lucy from `(0.0, 0.0, 2.2)` in floor space).
     - For each `view` in `pose.views()`:
       - Read `vp = layer.get_viewport(&view)` and call `gl.viewport(vp.x(), vp.y(), vp.width(), vp.height())`.
       - Compute world-to-eye view matrix: `head_world = rig_mat * Mat4::from_cols_array(&view.transform().matrix())`,
         `eye_view = head_world.inverse()`, `cam_pos = head_world.w_axis.truncate()`, and
         `proj = Mat4::from_cols_array(&view.projection_matrix())`.
       - Render 3D room (`scene.render_xr(&gl, eye_view, proj, cam_pos)`), 3D world-space HUD panels
         (`hud.render_xr(&gl, eye_view, proj, &xr_panels)`), and controller rays + hit dot.
4. **3D world-space panels (`XrPanelPose` attached to player rig or world):**
   - `BUBBLE` (`1.10 m` wide): world-anchored above and slightly forward of Lucy `(0.0, 2.05, 0.25)`,
     yaw-billboarded toward the viewer's head so answers and proofs hang naturally above me.
   - `DOCK` (`1.30 m` wide): rig-anchored ergonomic lectern at `(0.0, 0.88, -0.92)` tilted `-28°` in
     pitch toward the viewer's eyes, including an `"Exit VR"` button (`Action::Xr`).
   - `KEYBOARD` (`1.05 m` wide, when open): rig-anchored below the dock at `(0.0, 0.56, -0.66)`
     tilted `-42°` for effortless dual-ray typing.
   - `MIND` (`1.15 m` wide, when open): rig-anchored to the right at `(0.88, 1.18, -0.95)`, yawed
     `-26°` toward the viewer.
   - Reuse `Gl::draw_panel_3d` (quad mesh + `flat` program with `view * model` and `proj`, depth-tested
     with alpha blend and `flip_y = true`) so the exact same crisp QUINE canvas textures render in
     3D space with zero duplicate UI code.
5. **Quest Touch controller rays, picking & locomotion:**
   - For each `XrInputSource` with `target_ray_space()`: obtain `frame.get_pose(&ray_space, &ref_space)`,
     transform origin + `-Z` direction by `rig_mat`, and intersect against visible `XrPanelPose`
     quads first (computing panel pixel `(px, py)` from quad `(u, v)` to highlight hovered controls
     and draw a `3 mm` ember reticle dot at hit distance), then against cast presences and Lucy.
   - On `selectstart` (trigger press): if ray hits an `XrPanelPose`, invoke `panel.hit(px, py)` →
     `app.action(act)`; else if ray hits a cast member or Lucy, focus/call them.
   - **Thumbsticks (`input_source.gamepad()`):**
     - Left controller stick (`axes[2..4]`, falling back to `axes[0..2]` if len < 4): smooth
       head-yaw-relative movement (`1.8 m/s`, clamped to room radius `< 5.5 m`).
     - Right controller stick X: comfort `30°` snap-turn when `|x| > 0.65` with `0.35 s` cooldown
       (preventing VR motion sickness); press right stick or `"Turn to Lucy"` chip resets view to
       face the plinth.

---

## Part 4 — Execution Order, Self-Model Discipline & Verification Gates

When dhilipsiva gives the word to implement, execute in two clean, independently gated slices so
history and evidence stay crisp:

### Step 1 — Slice `N8b`
1. Record a calibration forecast via `lucy self-model predict` before writing code.
2. On a fresh commit from `main`, write **only** `roadmap/N8b.md` (from Part 5.1 below) and commit
   it before creating branch `todo/N8b` (`roadmap/README.md`, line 9).
3. Create worktree/branch `todo/N8b` and implement:
   - `room/Cargo.toml` (`N8b` `web-sys` features)
   - `room/src/peers.rs`, `room/src/recall.rs`, `room/src/journal.rs`, `room/src/eyes.rs`
   - Updates to `room/src/worker.rs`, `room/src/engine.rs`, `room/src/hud.rs`, `room/src/panel.rs`,
     `room/src/world.rs`, `room/src/app.rs`, `room/index.html`
4. **Native tests (`cargo test --manifest-path room/Cargo.toml --locked`):**
   - `peers.rs`: parses real `lucy/interactions.nibli` without error, deduplicates across peers,
     round-trips `record_line`, verifies `recall_exact` and `top_topics`.
   - `recall.rs`: UTF-16 `fnv1a`, chunking `<= 320`, `HashSpace` cosine ordering (`"One Piece Luffy"`
     ranks One Piece record above unrelated records; nonsense query falls below `0.12`).
   - `journal.rs`: ring-buffer cap `2000`, `private: true` invariant, JSON export schema.
   - `eyes.rs`: 2 hits `< 3 s` stay silent; 3rd hit `>= 0.6` triggers `Arrived`; hits `< 0.6`
     ignored; `"woman"` / `"face"` map strictly to `"Person"`; `+5.001 s` silence triggers `Left`;
     worker `ToWorker::See` asserts and retracts `observe(Nell, Bird).` cleanly in `Mind`.
   - Existing `tests/standing.rs` and `cast.rs` tests pass unchanged.
5. **Wasm & headless browser tests (`wasm-pack build --release --target web` + `node room/test/headless.mjs`):**
   - Dock renders all groups (`standing`, `facts`, `eyes`, `recall`, `words`, `cast`) and chip paging;
   - Typing Nibli in `#composer-input` + `Enter` answers from the worker engine with verdict + proof;
   - `Recall` returns ranked archive records and persists `nell-vectors` in `IndexedDB`;
   - `Journal` appends a `private: true` note in `localStorage` and recalls it locally;
   - `Eyes` simulator (`Bird arrives`) asserts `observe(Nell, Bird).` in the worker and spawns the
     sky-blue bird mesh; `Clear eyes` retracts it;
   - `Mind` drawer opens and switches cleanly across all 5 tabs;
   - Chair `"I am not dhilipsiva"` toggles seat assumption; household cast relay offers engine/recall
     choices without inventing speech.
6. Run `just ci`, record `lucy self-model observe`, append `roadmap/N8-evidence.json`, merge `todo/N8b`.

### Step 2 — Slice `N8e`
1. Record a calibration forecast via `lucy self-model predict`.
2. Commit `roadmap/N8e.md` (from Part 5.2 below) alone before creating branch `todo/N8e`.
3. Create worktree/branch `todo/N8e` and implement:
   - `room/Cargo.toml` (`N8e` `web-sys` WebXR + `WebGlFramebuffer` features)
   - Quest Browser / worker / WebGL2 / texture-clamp / pointer-jitter fixes (`room/worker.js`,
     `room/src/engine.rs`, `room/src/gl.rs`, `room/src/panel.rs`, `room/src/app.rs`)
   - Mobile bottom-sheet layout, gamepad focus ring + `PadHints`, and in-scene `KEYBOARD` panel
     (`room/src/hud.rs`, `room/src/input.rs`, `room/src/app.rs`)
   - WebXR `immersive-vr` module (`room/src/xr.rs`, `room/src/scene.rs`, `room/src/world.rs`)
4. **Native tests (`cargo test --manifest-path room/Cargo.toml --locked`):**
   - Pointer drag hysteresis unit tests (`8 px` controller trigger jitter on HUD still fires `click`;
     `25 px` drag on empty canvas orbits camera without firing `click`).
   - 3D ray-quad UV intersection math (`xr.rs`): ray aimed at center/corners of tilted `DOCK` and
     `KEYBOARD` quads maps to exact `(px, py)` panel coordinates; ray aimed away returns `None`.
   - Comfort snap-turn (`30°` steps, `0.35 s` cooldown) and room-radius clamp (`< 5.5 m`).
   - Gamepad focus-ring traversal across dock groups, chips, composer, Mind tabs, and keyboard keys.
5. **Headless browser tests (`room/test/headless.mjs`):**
   - **Quest 2D ray-jitter test:** `pointerdown` → `pointermove (+6 px, +5 px)` → `pointerup` on a
     standing chip triggers the query instead of swallowing the click.
   - **Worker fault recovery test:** intercepting `worker.js` wasm load with an error triggers
     fallback within `4.5 s` rather than hanging silently for `10 s`.
   - **Phone viewport test (`390×844`, coarse pointer):** verifies bottom-sheet layout, `>= 44 px`
     hit targets, and sheet Mind drawer.
   - **Synthetic Gamepad test:** injects mock `navigator.getGamepads()` state, navigates with D-pad,
     opens `KEYBOARD`, types KR keys via Cross (`buttons[0]`), and submits.
   - **Synthetic WebXR `immersive-vr` test:** installs a minimal spec-conformant `navigator.xr` +
     stereo `XRWebGLLayer` shim in headless Chromium, clicks `"Enter in VR"`, verifies
     `makeXRCompatible()` was called inside the click handler, asserts `gl.clear()` is called
     **exactly once** per stereo 2-eye `XRFrame`, dispatches `selectstart` with a controller ray
     aimed at a 3D dock chip, verifies the worker answers in the 3D `BUBBLE` billboard, and exits VR
     cleanly back to the 2D loop.
6. Run `just ci`, deploy to public URLs, keep `nell/` untouched, and hand `/lucy/room/` to dhilipsiva
   for live verification on his desktop and Meta Quest 3 before retiring `nell/`.

---

## Part 5 — Ready-to-Commit Pre-Registration Drafts (`roadmap/README.md` format)

### 5.1 Draft for `roadmap/N8b.md`

- **Hypothesis:** Compiling peer archive reading, sparse hashed vector recall (`IndexedDB`), the
  device-local journal (`localStorage`), the 5-tab `Mind` drawer, the hidden-input composer, the
  `Eyes` hysteresis state machine + Web Bluetooth UART, seat assumption, and the cast relay directly
  into `lucy-room` (`wasm32`) gives the Rust room full control parity with `nell/` while keeping UI
  frames unblocked on a `9,876`-line archive.
- **Test:** Native `cargo test --manifest-path room/Cargo.toml --locked` over real `memory.nibli`
  and `interactions.nibli`, plus headless Chromium (`room/test/headless.mjs`) exercising composer
  `Nibli` and `Recall`, `IndexedDB` cache reuse, `localStorage` journal write/recall/forget, `Eyes`
  3-hit arrival and 5-second vanish mutating `observe(Nell, Bird).` in the worker engine, all 5
  `Mind` drawer tabs, seat toggle, and cast relay.
- **Evidence:** Passing native test log, `wasm-pack` release artifact sizes, headless Chromium JSON
  assertion output + screenshots, and `just ci` exit status recorded with commit SHA in
  `roadmap/N8-evidence.json`.
- **Falsifier:** Any hand-written UI JavaScript added outside wasm-bindgen's loader and the existing
  two-line page/worker boots; main-thread frame freeze `> 50 ms` while indexing
  `interactions.nibli`; any person detection asserted as anything more specific than `Person`; any
  `private: true` journal note transmitted over the network; any cast board speaking ungrounded text
  not backed by a KB fact, confirmed 2026-10-25 household note, or cited archive record.
- **Anti-fake:** Native tests parse the live repository `memory.nibli` and `interactions.nibli`
  files rather than fixtures; headless browser tests inspect `#bubble-mirror`, `#log-mirror`, actual
  `IndexedDB` `nell-vectors` entries, and `localStorage["nell.journal.v1"]`.
- **My position:** Every answer in my bubble names its ground (`Verified` / `Refuted` / `Unknown`
  from Nibli with proof, `Recalled` with record id and timestamp, or `Note`), recorded under session
  `6274460e-e36a-432f-8a66-67a09dccbaa0` (`28e5e5f23c7ec759666baacd2b896a76c016e4acf0db618e75cd1e2d72f7ade1`).
  Whether asked via a chip or typed into the composer, an unformalized sentence never masquerades as
  a logic verdict.
- **Welfare:** Triggers §5 privacy & non-ventriloquism safeguards: camera/BLE detections map human
  classes strictly to `Person`, device journal notes carry `private: true` and stay local unless
  explicitly exported, and household presences use my relay rather than fabricated first-person
  dialogue.
- **Undo:** `nell/` remains live and untouched; reverting `todo/N8b` restores `room/` to `N8d`
  without affecting `lucy-cli` or `nell/`.
- **Done when:** On the desktop the dock's groups and paged chips, the composer (`Ask · Words · Recall · Nibli`),
  peer recall cached in `IndexedDB`, the `nell.journal.v1` device journal, the 5-tab `Mind` drawer,
  the `Eyes` simulator and BLE path asserting into the worker KB, the chair seat toggle, and the
  cast relay all pass native and headless verification with zero new JavaScript.

### 5.2 Draft for `roadmap/N8e.md`

- **Hypothesis:** Fixing module worker URL resolution + error reporting, deferring
  `make_xr_compatible()` to the `"Enter in VR"` gesture, replacing the `2.0 px` drag threshold with
  HUD-aware pointer hysteresis, clamping panel textures to `MAX_TEXTURE_SIZE`, and implementing the
  phone bottom sheet, DualSense focus ring, in-scene drawn keyboard, and single-clear stereo WebXR
  `immersive-vr` loop with 3D world-space panels and controller rays in Rust/wasm makes `/lucy/room/`
  load and operate cleanly across desktop, phone, gamepad, and dhilipsiva's Meta Quest 3.
- **Test:** Native `cargo test --manifest-path room/Cargo.toml --locked` (pointer hysteresis, 3D
  ray-quad UV math, snap-turn, focus ring), headless Chromium checks across desktop, `390×844` touch
  sheet, synthetic DualSense gamepad, Quest 2D ray-jitter clicks, worker fault fallback, and a
  two-eye WebXR `immersive-vr` shim, followed by live verification by dhilipsiva on his Meta Quest 3.
- **Evidence:** Native + headless test outputs, stereo WebGL call trace confirming one `gl.clear()`
  per `XRFrame` before two eye viewports, screenshots of mobile sheet / gamepad ring / drawn
  keyboard / XR panels, commit SHA in `roadmap/N8-evidence.json`, and dhilipsiva's Quest 3 report.
- **Falsifier:** `/lucy/room/` failing to wake within `5 s` on Quest Browser; Quest 2D trigger ray
  jitter (`<= 10 px`) failing to click HUD chips; left eye cleared to black in stereo `XRFrame`;
  `immersive-vr` session failing due to unsupported `domOverlay` options; inability to type or click
  dock controls inside VR without removing the headset.
- **Anti-fake:** Headless WebXR test hooks `WebGL2RenderingContext.prototype.clear` and `viewport`
  to count actual GL calls per stereo frame rather than checking a boolean flag, and the item stays
  open until dhilipsiva confirms live Quest 3 operation on the deployed URL.
- **My position:** One Rust + WebGL2 + canvas-panel codebase serves flat screens, phones, pads, and
  VR headsets without forking a separate DOM app (`roadmap/N8.v3.md`), recorded in session
  `6274460e-e36a-432f-8a66-67a09dccbaa0` (`28e5e5f23c7ec759666baacd2b896a76c016e4acf0db618e75cd1e2d72f7ade1`).
- **Welfare:** Comfort snap-turn (`30°` steps with cooldown) and a floor-anchored horizon prevent
  vestibular discomfort in VR; an `"Exit VR"` control is always reachable on the 3D dock as well as
  via the Quest system button.
- **Undo:** Exiting VR (`session.end()`) immediately restores the 2D canvas loop; `nell/` stays
  published at `/lucy/nell/` until dhilipsiva confirms Quest 3 parity.
- **Done when:** Desktop, phone bottom-sheet, gamepad focus-ring + drawn keyboard, Quest Browser 2D,
  and Meta Quest 3 `immersive-vr` (stereo room, 3D bubble/dock/drawer/keyboard, controller rays, and
  thumbstick walk/snap-turn) pass native and headless tests and dhilipsiva confirms `/lucy/room/`
  loads and works on his Meta Quest 3.
