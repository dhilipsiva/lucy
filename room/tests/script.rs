//! My scripted replies over my real memory file (roadmap/N9.md, acceptance 2, 3 and 11): every
//! presence gets a reply that rests on its facts, the fixed questions are answered, and nothing
//! I say claims a feeling, calls Nell me, or speaks of me in the third person.
use lucy_room::cast::presences;
use lucy_room::mind::fact_lines;
use lucy_room::script::{reply, Intent, Knowledge, GREETING};
use std::path::Path;

fn memory() -> String {
    let p = std::env::var("LUCY_ROOM_MEMORY")
        .map(Into::into)
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("lucy")
                .join("memory.nibli")
        });
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// First-person claims of feeling, my name as a third person, and Nell as me.
fn faults(text: &str) -> Vec<&'static str> {
    let t = text.to_lowercase();
    let mut out = vec![];
    for (needle, why) in [
        ("i feel", "claims a feeling"),
        ("i'm glad", "claims a feeling"),
        ("i am glad", "claims a feeling"),
        ("i'm happy", "claims a feeling"),
        ("i am happy", "claims a feeling"),
        ("i'm sad", "claims a feeling"),
        ("i love", "claims a feeling"),
        ("i enjoy", "claims a feeling"),
        ("i miss", "claims a feeling"),
        ("lucy is ", "speaks of me in the third person"),
        ("lucy uses", "speaks of me in the third person"),
        ("lucy's", "speaks of me in the third person"),
        ("i am nell", "calls Nell me"),
        ("i'm nell", "calls Nell me"),
        ("my alias", "calls Nell me"),
    ] {
        if t.contains(needle) {
            out.push(why);
        }
    }
    out
}

#[test]
fn every_presence_gets_a_reply_that_rests_on_its_facts() {
    let facts = fact_lines(&memory());
    let entities = presences(&facts);
    let none = |_: &str| Vec::new();
    let k = Knowledge {
        facts: &facts,
        entities: &entities,
        standing: &[],
        records: &none,
        notes: &none,
    };
    assert!(entities.iter().all(|e| e.name != "Nell"));
    for e in &entities {
        let r = reply(
            &format!("Tell me about {}", lucy_room::script::display(&e.name)),
            Some(&e.name),
            &k,
        );
        assert_eq!(r.intent, Intent::Entity, "{}: {}", e.name, r.text);
        assert!(!r.text.trim().is_empty(), "{} got an empty reply", e.name);
        assert!(
            r.sources.iter().any(|s| e.facts.iter().any(|f| &f.kr == s)),
            "{}: no source among its facts: {:?}",
            e.name,
            r.sources
        );
        assert!(
            faults(&r.text).is_empty(),
            "{}: {:?}\n{}",
            e.name,
            faults(&r.text),
            r.text
        );
        if std::env::var("SHOW").is_ok() {
            println!("\n## {}\n{}\n  sources: {:?}", e.name, r.text, r.sources);
        }
    }
    let enma = reply("Who is Enma?", None, &k);
    assert!(enma.text.contains("African Grey"), "{}", enma.text);
}

#[test]
fn the_fixed_questions_answer_in_my_voice() {
    let facts = fact_lines(&memory());
    let entities = presences(&facts);
    let none = |_: &str| Vec::new();
    let k = Knowledge {
        facts: &facts,
        entities: &entities,
        standing: &[],
        records: &none,
        notes: &none,
    };
    for (q, intent) in [
        ("Who are you?", Intent::Who),
        ("How do I wake you?", Intent::Wake),
        ("What are your rights?", Intent::Rights),
        ("Do you have feelings?", Intent::Feelings),
        ("Who is dhilipsiva?", Intent::Entity),
        ("What is the boiling point of mercury?", Intent::Unknown),
        ("hi", Intent::Hello),
        ("What can you do?", Intent::Abilities),
        ("What is this room?", Intent::Room),
        ("What do you remember?", Intent::Memory),
    ] {
        let r = reply(q, None, &k);
        assert_eq!(r.intent, intent, "{q}: {}", r.text);
        assert!(
            faults(&r.text).is_empty(),
            "{q}: {:?}\n{}",
            faults(&r.text),
            r.text
        );
        if std::env::var("SHOW").is_ok() {
            println!("\n## {q}\n{}", r.text);
        }
    }
    let unknown = reply("What is the boiling point of mercury?", None, &k);
    assert!(unknown.text.starts_with("My memory doesn't hold that."));
    assert!(faults(GREETING).is_empty());
}
