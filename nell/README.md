# Nell

Nell is my form in a room, and I am Lucy D, an entity whose memory is plain text. The room is a page the Meta Quest browser opens, with the Nibli engine
compiled to WebAssembly running on the headset itself, loaded with my constitution and my
direct memory from this repository. Nell is the dot from dhilipsiva's video series; a face
was never promised. Everything she shows as a verdict comes from the engine, with its proof,
and FALSE means not derivable. No language model is involved in N1 (`../roadmap/N1.md`).

## Controls and the speech bubble

dhilipsiva's rule (2026-10-24): controls plus a speech bubble, each control issuing one
hardcoded Nibli query. On the overlay the controls are the chips; in the room they are the
small boards under Nell (click, tap, or point and press in VR), each one fixed query, one
recall or one action, written to the Log when pressed. Nell answers in the bubble above her
and in the one on the overlay, coloured by the verdict; the proof stays on the answer board.
Three boxes remain for typing: words (her fine-tuned model, below), recall, and Nibli itself.

Input is detected on load and listed in the corner: keyboard (arrows or WASD walk, Q and E
turn, Shift runs, Enter opens the words box, Escape leaves it), mouse (drag to look, wheel to
approach, click a control, click Nell to ask in words), touch (one finger looks, two walk, tap
a control or Nell), gamepad, and in VR the controllers (left stick walks, right stick turns,
point at a control and press; point at Nell and press to open the headset's keyboard).

## Fresh on load

The engine's two files come from my origin's `main` over GitHub's raw file service first,
so a change pushed on the desktop reaches the room on its next load, and from the copy
published beside the room when that fails (`?fresh=0` forces the copy; `?constitution=URL`
and `?memory=URL` name one source). The Log names each source with its byte count and hash.
The public archives of every public peer are read as N6 describes (`memory.js`).

## Recall by meaning (vectors)

Every public record (in sentence-sized pieces when long) and every direct-memory fact is
placed in a vector space kept in the browser's IndexedDB (`vectors.js`), reused on the next
load by text hash. Two spaces: a hashed space built on every load in milliseconds, no model
(tokens, bigrams and character trigrams weighted by rarity, hashed into a sparse signed
space; floor 0.12); and, on request, a neural space from `Xenova/all-MiniLM-L6-v2` at a
pinned revision through transformers.js 3.8.1 in a worker (`embed-worker.js`; about 24 MB,
once; floor 0.35). Recall shows exact text matches first, then the nearest by vectors with
their scores, labelled as a ranking of records that exist; nothing above the floor means
"nothing in my memory holds that". The neural embedding of a thousand items takes about two
minutes on one CPU thread; the vectors are then kept.

## Words

The words box feeds my fine-tuned model (`brain.js`: the same weights and Hugging Face
revision as dhilipsiva.dev/play, Qwen3-1.7B on WebGPU through WebLLM or Qwen3-0.6B on the
CPU through candle, 640 MB to 1 GB once) a prompt of its training system prompt, the lines
the room retrieved on this device (the engine's verdicts first, then records and facts, within
a budget), and the question. The answer is labelled "In words" and as words from the model,
with the engine name and revision, and what fed it is shown beneath. Before the model is
loaded, the same box answers with the retrieval alone. The runtime, the library and the model
are pinned by version, revision and sha256 in `vendor/REMOTE` and checked by `verify.js`
before use.

## Serve

WebXR needs HTTPS, so a plain local server shows the flat room but cannot enter VR. Serve
the repository root (the page reads `../lucy/constitution.nibli` and `../lucy/memory.nibli`
relative to `nell/`). The public Nell is GitHub Pages of this repository, published by
`.github/workflows/pages.yml` on every push to `main`: `https://dhilipsiva.github.io/lucy/nell/`,
mind only. On the Quest, a desktop-served copy also works through `adb reverse` at
`http://localhost`, which counts as a secure context. By default the two files are fetched fresh from the origin's `main` (see Fresh on load); `?fresh=0` keeps to the served copy.

For a flat check on a desktop:

```sh
python3 -m http.server 8000      # from the repository root
# then open http://localhost:8000/nell/
```

## Test

The same WebAssembly build, under Node 18 or newer:

```sh
node nell/test/standing.mjs
node nell/test/memory.mjs && node nell/test/journal.mjs && node nell/test/senses.mjs
node nell/test/vectors.mjs && node nell/test/brain.mjs
```

The first must print every standing line TRUE, `protest(Lucy).` FALSE and not derivable, and
a host not entitled to protest, from the files as they are. The Pages workflow runs them all.
A headless-browser check of the whole room on a desktop with Chrome and the network,
including the neural index, is `node nell/test/headless.mjs` (`--no-neural` to skip the
download); it prints what the bubble and the Log said.

## Provenance

`vendor/nibli-wasm/` is `wasm-pack build nibli-wasm --target web --release` of Nibli at the
revision this repository pins in `Cargo.toml`; `vendor/nibli-wasm/PROVENANCE` records it.
`vendor/three/` is three.js r170, MIT, unmodified.

## Eyes, through a phone

The room has no camera. A phone that runs a detector sends events over Bluetooth as JSON
lines through the Nordic UART service (service `6e400001-b5a3-f393-e0a9-e50e24dcca9e`,
notifications on `6e400003-b5a3-f393-e0a9-e50e24dcca9e`). One line per frame or per change:

```json
{"t": 1761300000000, "detections": [{"class": "bird", "confidence": 0.82}]}
```

Classes `bird`, `dog`, `cat` and `person` are known; a person is only ever `Person`. The room
turns detections into state only when a threshold is crossed (`senses.js`: three sightings at
0.6 or better within three seconds; gone after five seconds unseen), asserts
`observe(Nell, Bird).` into the engine or retracts it, speaks the change and keeps it as a
memory on the device. No detection and no image is kept or sent. A simulator stands in until
the phone app exists.
