//! Lucy, her room: Lucy D's form in a WebGL2 room, all of it Rust compiled to WebAssembly
//! (roadmap/N8.md, N8.v2.md, N8.v3.md). The Nibli engine is compiled in; every verdict shown
//! is the engine's, with its proof. The modules without a browser in them (`mind`, `geom`,
//! `text`, `colors`, `util`, `cast`, `worker::handle`, `sources::from_query`) are tested natively on the same files the
//! browser fetches.

#![allow(clippy::too_many_arguments)]

pub mod app;
pub mod cast;
pub mod colors;
pub mod creatures;
pub mod engine;
pub mod geom;
pub mod gl;
pub mod hud;
pub mod input;
pub mod lucy;
pub mod mind;
pub mod mirrors;
pub mod panel;
pub mod scene;
pub mod sources;
pub mod text;
pub mod util;
pub mod worker;
pub mod world;

use wasm_bindgen::prelude::*;

/// The page's entry: the generated loader calls this once the module is instantiated. In the
/// worker there is no window; its script calls `worker_main` instead (roadmap/N8d.md).
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    if web_sys::window().is_none() {
        return;
    }
    if let Err(e) = app::boot() {
        web_sys::console::error_1(&JsValue::from_str(&format!(
            "Lucy's room could not start: {e}"
        )));
    }
}
