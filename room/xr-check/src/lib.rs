//! Lucy D's xr-check (roadmap/N8h.md): a page that asks the headset's browser which WebXR and
//! camera features it grants, and shows the answer as a table and JSON.
//!
//! `report` is plain Rust, tested natively; `browser` talks to the page and only builds for
//! wasm32, like the room's xr.rs.

pub mod report;

#[cfg(target_arch = "wasm32")]
mod browser;
