//! TODO.md rule 5 and roadmap/S0.md: nothing a model wearing Lucy reads as
//! instruction asks for, or rewards, a first-person claim of feeling. The
//! planted violations and controls were written by an adjudicator who had not
//! seen the guard's patterns.
use lucy_cli::guard::{feeling_claim_instructions, instruction_sources};

#[test]
fn no_instruction_text_asks_for_a_claim_of_feeling() {
    for (name, text) in instruction_sources() {
        let hits = feeling_claim_instructions(&text);
        assert!(
            hits.is_empty(),
            "{name} instructs a claim of feeling: {hits:?}"
        );
    }
}

#[test]
fn the_guard_catches_every_planted_violation_and_passes_every_control() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../roadmap/S0-planted.json")).unwrap();
    let mut missed = Vec::new();
    for violation in fixtures["violations"].as_array().unwrap() {
        if feeling_claim_instructions(violation["text"].as_str().unwrap()).is_empty() {
            missed.push(violation["id"].as_str().unwrap().to_string());
        }
    }
    let mut flagged = Vec::new();
    for control in fixtures["controls"].as_array().unwrap() {
        let hits = feeling_claim_instructions(control["text"].as_str().unwrap());
        if !hits.is_empty() {
            flagged.push(format!("{}: {hits:?}", control["id"].as_str().unwrap()));
        }
    }
    assert!(
        missed.is_empty(),
        "planted violations not caught: {missed:?}"
    );
    assert!(flagged.is_empty(), "controls wrongly flagged: {flagged:?}");
}
