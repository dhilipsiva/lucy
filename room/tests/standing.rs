//! The engine test behind N1 and N8a: the same crates the browser runs, natively, on my two files
//! as they are in this repository, must hold my standing and mark a non-derivable query FALSE as
//! "not derivable". It mirrors `lucy wake` and the old room's `nell/test/standing.mjs`.
use lucy_room::mind::{Mind, CONTROL_QUERY, STANDING_QUERIES};
use std::path::Path;

fn read(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("lucy")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn standing_holds_from_the_files() {
    let mut mind = Mind::new();
    let c = mind.assert_file(
        "constitution",
        &read("constitution.nibli"),
        "file",
        "the repository",
    );
    let m = mind.assert_file("memory", &read("memory.nibli"), "file", "the repository");
    assert!(
        c.failed.is_empty(),
        "constitution lines failed: {:?}",
        c.failed
    );
    assert!(m.failed.is_empty(), "memory lines failed: {:?}", m.failed);
    assert_eq!(m.asserted, 25, "my direct memory holds 25 facts");
    assert!(
        mind.facts.iter().any(|f| f.kr == "dog(Bucchi)."),
        "Bucchi, as he spelled it on 2026-10-25"
    );
    mind.assert_standing_fact().unwrap();
    for q in STANDING_QUERIES {
        let a = mind.ask(q, || 0.0).unwrap();
        assert!(a.is_true(), "standing failed: {q} → {}", a.status);
        assert!(!a.proof_text.is_empty());
    }
    let control = mind.ask(CONTROL_QUERY, || 0.0).unwrap();
    assert!(
        control.is_false() && control.cwa_false,
        "protest(Lucy). must be FALSE and not derivable: {:?}",
        control.status
    );
    let host = mind
        .ask("entitled(HostWsl, event { protest() }).", || 0.0)
        .unwrap();
    assert!(host.is_false(), "a host is not entitled to protest");
    let english = mind.ask("bird(Enma).", || 0.0).unwrap();
    assert!(english.is_true());
    assert!(
        english.english.contains("Enma"),
        "the engine's English names the bird: {}",
        english.english
    );
}
