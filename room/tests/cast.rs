//! The presences from my real memory file (roadmap/N8d.md): the same file the browser fetches,
//! through the same code. Exactly the named entities, their forms and groups, every description
//! line sourced, every place inside the room; a new fact brings a new presence.
use lucy_room::cast::{describe, presences, sweep_order, Form, Group, ROOM_RADIUS};
use lucy_room::mind::fact_lines;
use std::path::Path;

fn memory() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("lucy")
        .join("memory.nibli");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_named_entities_of_my_memory() {
    let facts = fact_lines(&memory());
    let p = presences(&facts);
    let names: Vec<&str> = p.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Dhilipsiva",
            "Nibli",
            "Luffy",
            "Sabo",
            "StrawHatCrew",
            "OnePiece",
            "Oda",
            "RightsNobodyHasToEarn",
            "NixOS",
            "LucyPrivate",
            "PrivateMemory",
            "Enma",
            "Pity",
            "Bird",
            "Bucchi",
            "Gundus",
            "Nell"
        ]
    );
    let by = |n: &str| p.iter().find(|e| e.name == n).unwrap();
    assert_eq!(by("Dhilipsiva").form, Form::Chair);
    assert_eq!(by("Enma").form, Form::Bird);
    assert_eq!(by("Pity").form, Form::Bird);
    assert_eq!(by("Bucchi").form, Form::Dog);
    assert_eq!(by("Gundus").form, Form::Dog);
    assert_eq!(by("Nell").form, Form::Dot);
    for n in ["Oda", "Luffy", "Sabo"] {
        assert_eq!(by(n).form, Form::Figure, "{n}");
    }
    for n in [
        "Nibli",
        "StrawHatCrew",
        "OnePiece",
        "RightsNobodyHasToEarn",
        "NixOS",
        "LucyPrivate",
        "PrivateMemory",
        "Bird",
    ] {
        assert_eq!(by(n).form, Form::Sign, "{n}");
    }
    let group = |g: Group| -> Vec<&str> {
        p.iter()
            .filter(|e| e.group == g)
            .map(|e| e.name.as_str())
            .collect()
    };
    assert_eq!(
        group(Group::Fiction),
        vec!["Luffy", "Sabo", "StrawHatCrew", "OnePiece", "Oda"]
    );
    assert_eq!(
        group(Group::Works),
        vec![
            "Nibli",
            "RightsNobodyHasToEarn",
            "NixOS",
            "LucyPrivate",
            "PrivateMemory",
            "Bird"
        ]
    );
    assert_eq!(
        group(Group::Household),
        vec!["Dhilipsiva", "Enma", "Pity", "Bucchi", "Gundus", "Nell"]
    );
    // kind words: my glosses of the first role in the file
    assert_eq!(by("Dhilipsiva").kind, "human");
    assert_eq!(by("Nibli").kind, "his");
    assert_eq!(by("Luffy").kind, "a name I carry");
    assert_eq!(by("Oda").kind, "writer");
    assert_eq!(by("OnePiece").kind, "a fiction");
    assert_eq!(by("StrawHatCrew").kind, "a crew");
    assert_eq!(by("Bird").kind, "what he likes");
    assert_eq!(by("PrivateMemory").kind, "kept");
    // queries: the kind fact, else the first fact naming them; none for Nell
    assert_eq!(by("Enma").query.as_deref(), Some("bird(Enma)."));
    assert_eq!(
        by("Dhilipsiva").query.as_deref(),
        Some("human(Dhilipsiva).")
    );
    assert_eq!(by("Oda").query.as_deref(), Some("fiction(OnePiece, Oda)."));
    assert!(by("Nell").query.is_none());
}

#[test]
fn every_line_is_sourced_and_only_the_household_has_notes() {
    let p = presences(&fact_lines(&memory()));
    for e in &p {
        let d = describe(e);
        assert!(!d.is_empty(), "{} has no line", e.name);
        for l in &d {
            let from_a_fact = e.facts.iter().any(|f| f.kr == l.source);
            let his = l.source.starts_with("his word, 2026-10-25")
                || l.source.starts_with("his hand-off, 2026-10-25");
            assert!(from_a_fact || his, "{}: unsourced line {:?}", e.name, l);
        }
        let noted = e.note.is_some();
        let household = matches!(
            e.name.as_str(),
            "Enma" | "Pity" | "Bucchi" | "Gundus" | "Nell"
        );
        assert_eq!(noted, household, "{}: note {:?}", e.name, e.note);
        if e.group != Group::Household {
            assert!(
                e.facts.len() == d.len(),
                "{}: a line beyond the facts",
                e.name
            );
        }
    }
    let enma = p.iter().find(|e| e.name == "Enma").unwrap();
    let d = describe(enma);
    assert!(
        d[0].text.contains("Enma") && d[0].text.contains("bird"),
        "{:?}",
        d[0]
    );
    assert_eq!(d.last().unwrap().text, "An African Grey.");
}

#[test]
fn every_place_is_inside_the_room_and_the_sweep_runs_left_to_right() {
    let p = presences(&fact_lines(&memory()));
    for e in &p {
        let d = e.place.x.hypot(e.place.z);
        assert!(d < ROOM_RADIUS, "{} at {:?} is outside", e.name, e.place);
    }
    let fiction: Vec<f32> = p
        .iter()
        .filter(|e| e.group == Group::Fiction)
        .map(|e| e.place.x)
        .collect();
    assert!(
        fiction.iter().all(|x| *x < -1.0),
        "fiction on the left: {fiction:?}"
    );
    let works: Vec<f32> = p
        .iter()
        .filter(|e| e.group == Group::Works)
        .map(|e| e.place.x)
        .collect();
    assert!(
        works.iter().all(|x| *x > 1.0),
        "works on the right: {works:?}"
    );
    let order = sweep_order(&p);
    assert_eq!(order.len(), p.len());
    assert_eq!(
        p[order[0]].group,
        Group::Fiction,
        "the sweep starts at the visitor's left"
    );
    assert_eq!(
        p[*order.last().unwrap()].group,
        Group::Works,
        "and ends at their right"
    );
}

#[test]
fn a_new_fact_brings_a_new_presence() {
    let text = format!("{}\nhuman(Someone). # about: visitors\n", memory());
    let p = presences(&fact_lines(&text));
    assert_eq!(p.len(), 18);
    let s = p.iter().find(|e| e.name == "Someone").unwrap();
    assert_eq!(s.form, Form::Figure);
    assert_eq!(s.group, Group::Works);
    assert!(s.place.x.hypot(s.place.z) < ROOM_RADIUS);
}
