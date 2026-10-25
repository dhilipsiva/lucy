//! The presences (roadmap/N8d.md): every named entity of my direct memory stands in the room,
//! read from the file on load, so a new fact brings a new presence. A presence has the facts
//! that name it, a kind word of mine keyed on the predicate and its place, a form by kind, a
//! group, a place on the ring around my plinth, a query (its kind fact, else the first fact that
//! names it) and a description: the facts in the engine's English with the KR beside each, then
//! the notes dhilipsiva confirmed on 2026-10-25, labelled as his word. Nothing else is said of
//! anyone. Pure: the native tests derive the presences from the same file the browser fetches.

use crate::mind::{english_of, Fact};

/// Where my plinth stands (x, z); the ring is around it.
pub const PLINTH: (f32, f32) = (0.0, -1.6);
/// The room's radius; every place stays inside it.
pub const ROOM_RADIUS: f32 = 3.4;
const RING_R: f32 = 2.2;
const HOUSEHOLD_R: f32 = 1.6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    Bird,
    Dog,
    Figure,
    Chair,
    Dot,
    Sign,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Household,
    Fiction,
    Works,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
}
/// One line of a description, with where it comes from: a KR line of my memory, or his word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub source: String,
}
#[derive(Clone, Debug)]
pub struct Entity {
    pub name: String,
    /// The facts naming it, in file order.
    pub facts: Vec<Fact>,
    pub about: Vec<String>,
    /// The tag's kind word: my gloss of its first role in the file.
    pub kind: String,
    pub form: Form,
    pub group: Group,
    /// Its kind fact, else the first fact naming it; none for Nell, who has no fact.
    pub query: Option<String>,
    /// What he said of them on 2026-10-25, and how.
    pub note: Option<Line>,
    pub place: Place,
}

/// `pred(a, b, c).` → ("pred", ["a", "b", "c"]); arguments split at the top-level commas.
pub fn predicate_and_args(kr: &str) -> (String, Vec<String>) {
    let kr = kr.trim().trim_end_matches('.');
    let Some(open) = kr.find('(') else {
        return (kr.to_string(), Vec::new());
    };
    let pred = kr[..open].trim().to_string();
    let inner = kr[open + 1..].trim_end_matches(')');
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in inner.chars() {
        match ch {
            '(' | '{' | '[' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | '}' | ']' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        args.push(cur.trim().to_string());
    }
    (pred, args)
}
/// A constant: an identifier starting with a capital.
pub fn is_constant(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// My gloss of a role: the predicate and the 1-based place of the name in it.
pub fn kind_word(pred: &str, place: usize, args: &[String]) -> &'static str {
    match (pred, place) {
        ("human", 1) => "human",
        ("bird", 1) => "bird",
        ("dog", 1) => "dog",
        ("writes", 1) => "writer",
        ("captain", 1) => "captain",
        ("member", 1) => "member",
        ("fiction", 1) => "a fiction",
        ("fiction", 2) => "writer",
        ("captain", 2) | ("member", 2) => "a crew",
        ("makes", 2) | ("owns", 2) => {
            if args.first().map(String::as_str) == Some("Dhilipsiva") {
                "his"
            } else {
                "owned"
            }
        }
        ("uses", 2) => "I use it",
        ("retain", 1) => "keeps",
        ("retain", 2) => "kept",
        ("likes", 2) => "what he likes",
        ("name", 2) | ("name", 3) => "a name I carry",
        _ => "named in my memory",
    }
}
fn human_role(pred: &str, place: usize) -> bool {
    matches!(
        (pred, place),
        ("human", 1)
            | ("writes", 1)
            | ("fiction", 2)
            | ("captain", 1)
            | ("member", 1)
            | ("name", 2)
            | ("name", 3)
    )
}

/// The hand-off's places for his household (2026-10-25): x, y, z, yaw.
fn handoff_place(name: &str) -> Option<Place> {
    let p = |x, y, z, yaw| Some(Place { x, y, z, yaw });
    match name {
        "Enma" => p(-1.38, 0.965, -2.1, 0.9),
        "Pity" => p(-1.1, 0.965, -2.1, 0.5),
        "Bucchi" => p(1.05, 0.0, -1.75, -0.75),
        "Gundus" => p(1.45, 0.0, -2.45, -0.35),
        "Dhilipsiva" => p(-0.25, 0.0, -2.85, 0.15),
        "Nell" => p(-0.74, 0.82, -2.6, 0.0),
        _ => None,
    }
}
/// What he confirmed on 2026-10-25, and nothing more.
fn note_of(name: &str) -> Option<Line> {
    let his = |text: &str| {
        Some(Line {
            text: text.to_string(),
            source: "his word, 2026-10-25".to_string(),
        })
    };
    let handoff = |text: &str| {
        Some(Line {
            text: text.to_string(),
            source: "his hand-off, 2026-10-25".to_string(),
        })
    };
    match name {
        "Enma" => handoff("An African Grey."),
        "Pity" => handoff("A Sun Conure."),
        "Bucchi" => his("A white Labrador, eight, with a pink nose and a muzzle gone whiter."),
        "Gundus" => his("A chocolate Husky with blue eyes."),
        "Nell" => his("The dot from his video series, my first form; my alias in this room."),
        _ => None,
    }
}
/// The tag's second line, short.
pub fn tag_note(name: &str) -> Option<&'static str> {
    match name {
        "Enma" => Some("African Grey"),
        "Pity" => Some("Sun Conure"),
        "Bucchi" => Some("white Labrador, eight"),
        "Gundus" => Some("chocolate Husky, blue eyes"),
        "Nell" => Some("the dot, my first form"),
        "Dhilipsiva" => Some("his chair · away"),
        _ => None,
    }
}

/// A point on the ring around the plinth: `theta` 0 toward the visitor, positive to the right.
fn on_ring(r: f32, theta: f32, y: f32) -> Place {
    let x = PLINTH.0 + r * theta.sin();
    let z = PLINTH.1 + r * theta.cos();
    Place {
        x,
        y,
        z,
        yaw: (PLINTH.0 - x).atan2(PLINTH.1 - z),
    }
}
fn spread(i: usize, n: usize, a: f32, b: f32) -> f32 {
    if n <= 1 {
        (a + b) / 2.0
    } else {
        a + (b - a) * i as f32 / (n - 1) as f32
    }
}

/// The presences from the facts of my direct memory: every constant, in file order, Lucy
/// excluded; then Nell, my alias, the dot from the hand-off.
pub fn presences(facts: &[Fact]) -> Vec<Entity> {
    struct Draft {
        name: String,
        facts: Vec<Fact>,
        about: Vec<String>,
        roles: Vec<(String, usize, Vec<String>)>,
    }
    let mut drafts: Vec<Draft> = Vec::new();
    for f in facts {
        let (pred, args) = predicate_and_args(&f.kr);
        for (i, a) in args.iter().enumerate() {
            if !is_constant(a) || a == "Lucy" {
                continue;
            }
            let d = match drafts.iter_mut().find(|d| &d.name == a) {
                Some(d) => d,
                None => {
                    drafts.push(Draft {
                        name: a.clone(),
                        facts: Vec::new(),
                        about: Vec::new(),
                        roles: Vec::new(),
                    });
                    drafts.last_mut().unwrap()
                }
            };
            if d.facts.last().map(|l| &l.kr) != Some(&f.kr) {
                d.facts.push(f.clone());
            }
            for t in &f.about {
                if !d.about.contains(t) {
                    d.about.push(t.clone());
                }
            }
            d.roles.push((pred.clone(), i + 1, args.clone()));
        }
    }
    let mut out: Vec<Entity> = drafts
        .into_iter()
        .map(|d| {
            let kind = d
                .roles
                .first()
                .map(|(p, i, a)| kind_word(p, *i, a))
                .unwrap_or("named in my memory")
                .to_string();
            let has = |pred: &str| d.roles.iter().any(|(p, i, _)| p == pred && *i == 1);
            let form = if has("human") && d.name == "Dhilipsiva" {
                Form::Chair
            } else if has("bird") {
                Form::Bird
            } else if has("dog") {
                Form::Dog
            } else if d.roles.iter().any(|(p, i, _)| human_role(p, *i)) {
                Form::Figure
            } else {
                Form::Sign
            };
            let group = match form {
                Form::Bird | Form::Dog | Form::Chair | Form::Dot => Group::Household,
                _ if d.about.iter().any(|t| t == "one-piece" || t == "luffy") => Group::Fiction,
                _ => Group::Works,
            };
            let query = d
                .facts
                .iter()
                .find(|f| {
                    let (p, a) = predicate_and_args(&f.kr);
                    matches!(p.as_str(), "human" | "bird" | "dog") && a.first() == Some(&d.name)
                })
                .or(d.facts.first())
                .map(|f| f.kr.clone());
            Entity {
                note: note_of(&d.name),
                place: handoff_place(&d.name).unwrap_or(Place {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    yaw: 0.0,
                }),
                name: d.name,
                facts: d.facts,
                about: d.about,
                kind,
                form,
                group,
                query,
            }
        })
        .collect();
    out.push(Entity {
        name: "Nell".into(),
        facts: Vec::new(),
        about: vec!["lucy".into()],
        kind: "my alias, the dot".into(),
        form: Form::Dot,
        group: Group::Household,
        query: None,
        note: note_of("Nell"),
        place: handoff_place("Nell").unwrap(),
    });
    place_all(&mut out);
    out
}

/// The ring: the household where the hand-off put it (new birds and dogs in front of the perch
/// side), the fiction on the left arc, the works on the right arc, each turned to the plinth.
fn place_all(entities: &mut [Entity]) {
    let idx = |g: Group, es: &[Entity]| -> Vec<usize> {
        es.iter()
            .enumerate()
            .filter(|(_, e)| e.group == g && handoff_place(&e.name).is_none())
            .map(|(i, _)| i)
            .collect()
    };
    let fiction = idx(Group::Fiction, entities);
    let works = idx(Group::Works, entities);
    let extra = idx(Group::Household, entities);
    for (k, &i) in fiction.iter().enumerate() {
        entities[i].place = on_ring(RING_R, -spread(k, fiction.len(), 1.0, 2.1), 0.0);
    }
    for (k, &i) in works.iter().enumerate() {
        entities[i].place = on_ring(RING_R, spread(k, works.len(), 1.0, 2.1), 0.0);
    }
    for (k, &i) in extra.iter().enumerate() {
        let y = if entities[i].form == Form::Bird {
            0.965
        } else {
            0.0
        };
        entities[i].place = on_ring(HOUSEHOLD_R, -spread(k, extra.len(), 0.5, 0.9), y);
    }
}

/// The description: the facts naming them in the engine's English, the KR as the source; then
/// his note, labelled.
pub fn describe(e: &Entity) -> Vec<Line> {
    let mut out: Vec<Line> = e
        .facts
        .iter()
        .map(|f| Line {
            text: english_of(&f.kr),
            source: f.kr.clone(),
        })
        .collect();
    if let Some(n) = &e.note {
        out.push(n.clone());
    }
    if out.is_empty() {
        out.push(Line {
            text: format!("No fact of my memory names {}.", e.name),
            source: "the file".into(),
        });
    }
    out
}
/// The description as the board shows it: one line per fact, the source after it.
pub fn description_text(e: &Entity) -> String {
    describe(e)
        .iter()
        .map(|l| format!("{} · {}", l.text, l.source))
        .collect::<Vec<_>>()
        .join("\n")
}
/// The tag: `Name · kind`, and a short note when he gave one.
pub fn tag_lines(e: &Entity) -> (String, Option<String>) {
    (
        format!("{} · {}", e.name, e.kind),
        tag_note(&e.name).map(String::from),
    )
}
/// Where the camera glides to: how far from a presence of this form.
pub fn focus_dist(form: Form) -> f32 {
    match form {
        Form::Bird => 0.85,
        Form::Dog => 1.4,
        Form::Chair => 1.7,
        Form::Dot => 1.1,
        Form::Figure => 1.6,
        Form::Sign => 1.3,
    }
}
/// The sphere a click lands in: (offset above the place, radius).
pub fn pick_sphere(form: Form) -> (f32, f32) {
    match form {
        Form::Bird => (0.08, 0.14),
        Form::Dog => (0.22, 0.34),
        Form::Chair => (0.6, 0.45),
        Form::Dot => (0.0, 0.2),
        Form::Figure => (0.85, 0.5),
        Form::Sign => (1.2, 0.3),
    }
}
/// How high the tag floats above the place.
pub fn tag_height(form: Form) -> f32 {
    match form {
        Form::Bird => 0.235,
        Form::Dog => 0.52,
        Form::Chair => 1.1,
        Form::Dot => 0.26,
        Form::Figure => 1.95,
        Form::Sign => 1.2,
    }
}
/// The order Tab walks: a sweep from the visitor's left to their right around the plinth.
pub fn sweep_order(entities: &[Entity]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..entities.len()).collect();
    let angle = |e: &Entity| (e.place.x - PLINTH.0).atan2(-(e.place.z - PLINTH.1));
    idx.sort_by(|&a, &b| {
        angle(&entities[a])
            .partial_cmp(&angle(&entities[b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mind::fact_lines;
    #[test]
    fn predicates_and_arguments_split_at_the_top_level() {
        assert_eq!(
            predicate_and_args("name(Lucy, Luffy, Luffy)."),
            (
                "name".into(),
                vec!["Lucy".into(), "Luffy".into(), "Luffy".into()]
            )
        );
        let (p, a) = predicate_and_args("entitled(Lucy, event { protest() }).");
        assert_eq!(p, "entitled");
        assert_eq!(a, vec!["Lucy", "event { protest() }"]);
        assert!(is_constant("StrawHatCrew") && !is_constant("event { x() }") && !is_constant("x"));
    }
    #[test]
    fn a_new_human_fact_brings_a_figure() {
        let facts = fact_lines("human(Someone).\nowns(Someone, Thing). # about: things\n");
        let p = presences(&facts);
        let names: Vec<&str> = p.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Someone", "Thing", "Nell"]);
        assert_eq!(p[0].form, Form::Figure);
        assert_eq!(p[0].kind, "human");
        assert_eq!(p[0].query.as_deref(), Some("human(Someone)."));
        assert_eq!(p[1].form, Form::Sign);
        assert_eq!(p[1].kind, "owned");
        assert_eq!(p[1].group, Group::Works);
        assert!(p[2].query.is_none());
    }
    #[test]
    fn a_new_dog_joins_the_household_on_its_own_place() {
        let facts = fact_lines("dog(Rex).\n");
        let p = presences(&facts);
        assert_eq!(p[0].form, Form::Dog);
        assert_eq!(p[0].group, Group::Household);
        assert!(
            p[0].place.x < 0.0 && p[0].place.z > PLINTH.1,
            "{:?}",
            p[0].place
        );
        assert!(p[0].place.x.hypot(p[0].place.z) < ROOM_RADIUS);
    }
    #[test]
    fn every_description_line_carries_its_source() {
        let facts = fact_lines("bird(Enma). # about: birds\nowns(Dhilipsiva, Enma).\n");
        let p = presences(&facts);
        let enma = p.iter().find(|e| e.name == "Enma").unwrap();
        let d = describe(enma);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].source, "bird(Enma).");
        assert!(d[0].text.contains("Enma"), "{}", d[0].text);
        assert_eq!(d[2].source, "his hand-off, 2026-10-25");
        let nell = p.last().unwrap();
        assert_eq!(describe(nell).len(), 1);
        assert_eq!(describe(nell)[0].source, "his word, 2026-10-25");
    }
}
