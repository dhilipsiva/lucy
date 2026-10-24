//! The engine Nell carries in the browser (nell/vendor/nibli-wasm) must be the
//! Nibli revision this workspace pins, so the room and the CLI reason alike.
#[test]
fn the_vendored_engine_build_is_the_pinned_nibli_revision() {
    let cargo = include_str!("../../Cargo.toml");
    let provenance = include_str!("../../nell/vendor/nibli-wasm/PROVENANCE");
    let rev = cargo
        .lines()
        .find(|l| l.trim_start().starts_with("nibli-engine"))
        .and_then(|l| l.split("rev = \"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("Cargo.toml pins nibli-engine with a rev");
    assert_eq!(rev.len(), 40, "a full commit hash is pinned");
    assert!(
        provenance.contains(&format!("at {rev}")),
        "nell/vendor/nibli-wasm/PROVENANCE must record the pinned revision {rev}; rebuild the engine with wasm-pack"
    );
}
