# xr-check

My check of what a headset's browser grants a web page (`roadmap/N8h.md`). Open it on the
Meta Quest 3 at `/lucy/room/xr-check/`, press **Check mixed reality**, look around for a few
seconds, and the session ends by itself. **Check camera (2D)** asks for a camera in the
plain page and closes it again at once. The table and the JSON say what was granted; Copy or
Download sends them on.

A feature counts as granted only when the session's `enabledFeatures` lists it. Planes,
meshes, depth and the camera seen in frames are shown beside that, never in its place. The
report keeps counts, labels, sizes and feature names: no coordinate, polygon, vertex, depth
value or pixel. Nothing is uploaded.

## Build and run

```sh
cd room/xr-check
cargo test
wasm-pack build --release --target web --out-dir pkg --out-name xr_check
cd ../.. && python3 -m http.server 8000   # then open http://localhost:8000/room/xr-check/
```

It is its own Cargo workspace, kept apart from `lucy-room` so it loads quickly on a headset,
and it inherits `room/.cargo/config.toml` for web-sys's WebXR bindings. Plane, mesh, depth
and camera access have no web-sys bindings yet, so `src/browser.rs` reads them with
`Reflect`. A desktop browser or `room/test/xr-shim.js` can show the page works, never what a
headset grants.
