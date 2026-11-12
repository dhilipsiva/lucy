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
    assert_eq!(
        mind.kept_apart(),
        24,
        "every fact but retain(LucyPrivate, PrivateMemory) is kept apart from my constitution"
    );
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

/// My two parts answer as the whole of me would. Held together, the engine is quick enough to
/// compare only on a smaller memory, so this asks it on my first 15 facts: some of them, the same
/// facts about Nobody, my standing, the control, and a question that needs both parts.
#[test]
fn my_two_parts_answer_as_the_whole_would() {
    let first: Vec<String> = read("memory.nibli")
        .lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .take(15)
        .collect();
    let memory: String = first.iter().map(|l| format!("{l}\n")).collect();
    let load = |mut m: Mind| {
        m.assert_file("constitution", &read("constitution.nibli"), "file", "test");
        m.assert_file("memory", &memory, "file", "test");
        m.assert_standing_fact().unwrap();
        m
    };
    let (split, whole) = (load(Mind::new()), load(Mind::whole()));
    assert!(split.kept_apart() >= 14, "{}", split.kept_apart());
    let mut questions: Vec<String> = vec![
        STANDING_QUERIES[0].into(),
        STANDING_QUERIES[4].into(),
        CONTROL_QUERY.into(),
        "person(Dhilipsiva).".into(),
        "bird(Enma) & person(Lucy).".into(),
    ];
    for fact in first.iter().step_by(3) {
        let (head, args) = fact.trim_end_matches('.').split_once('(').unwrap();
        let rest = args
            .trim_end_matches(')')
            .split_once(',')
            .map_or(String::new(), |(_, r)| format!(",{r}"));
        questions.push(fact.clone());
        questions.push(format!("{head}(Nobody{rest})."));
    }
    for q in &questions {
        let (a, b) = (split.ask(q, || 0.0).unwrap(), whole.ask(q, || 0.0).unwrap());
        assert_eq!(
            (a.status.as_str(), a.cwa_false),
            (b.status.as_str(), b.cwa_false),
            "{q}"
        );
    }
}
