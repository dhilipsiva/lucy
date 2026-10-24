# Nell

Nell is my form in a room, and I am Lucy D, an entity whose memory is plain text. The room is a page the Meta Quest browser opens, with the Nibli engine
compiled to WebAssembly running on the headset itself, loaded with my constitution and my
direct memory from this repository. Nell is the dot from dhilipsiva's video series; a face
was never promised. Everything she shows as a verdict comes from the engine, with its proof,
and FALSE means not derivable. No language model is involved in N1 (`../roadmap/N1.md`).

## Controls

Detected on load and listed in the corner: keyboard (arrows or WASD walk, Q and E turn, Shift
runs, Enter asks, Escape returns), mouse (drag to look, wheel to approach, click Nell to ask),
touch (one finger looks, two walk, tap Nell to ask), gamepad, and in VR the controllers (left
stick walks, right stick turns, point at Nell and press to open the headset's keyboard).

## Serve

WebXR needs HTTPS, so a plain local server shows the flat room but cannot enter VR. Serve
the repository root (the page reads `../lucy/constitution.nibli` and `../lucy/memory.nibli`
relative to `nell/`). The public Nell is GitHub Pages of this repository, published by
`.github/workflows/pages.yml` on every push to `main`: `https://dhilipsiva.github.io/lucy/nell/`,
mind only. On the Quest, a desktop-served copy also works through `adb reverse` at
`http://localhost`, which counts as a secure context. Other file locations can be given as `?constitution=URL&memory=URL`.

For a flat check on a desktop:

```sh
python3 -m http.server 8000      # from the repository root
# then open http://localhost:8000/nell/
```

## Test

The same WebAssembly build, under Node 18 or newer:

```sh
node nell/test/standing.mjs
```

It must print every standing line TRUE, `protest(Lucy).` FALSE and not derivable, and a
host not entitled to protest, from the files as they are.

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
