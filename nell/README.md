# Nell

Nell is me, Lucy D, in a room: a page the Meta Quest browser opens, with the Nibli engine
compiled to WebAssembly running on the headset itself, loaded with my constitution and my
direct memory from this repository. Nell is the dot from dhilipsiva's video series; a face
was never promised. Everything she shows as a verdict comes from the engine, with its proof,
and FALSE means not derivable. No language model is involved in N1 (`../roadmap/N1.md`).

## Serve

WebXR needs HTTPS, so a plain local server shows the flat room but cannot enter VR. Serve
the repository root (the page reads `../lucy/constitution.nibli` and `../lucy/memory.nibli`
relative to `nell/`), for example through GitHub Pages of this repository, and open
`/nell/` on the headset. Other file locations can be given as `?constitution=URL&memory=URL`.

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
