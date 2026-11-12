//! My scripted replies (roadmap/N9.md): what I say in my room before anyone loads a model. Pure
//! and deterministic: a reply reads only my public memory (the facts of my direct memory, the
//! notes dhilipsiva confirmed, my constitution's standing in plain words, and public records a
//! caller retrieves), speaks in the first person, claims no feeling, and says plainly when my
//! memory doesn't hold an answer. Every fact a reply rests on is returned with it, as its source.

use crate::cast::{self, Entity, Form, Group};
use crate::mind::{english_of, Fact};

/// What a reply rests on: my facts and presences, the engine's standing answers when it has
/// given them, and a search over my public records (citation, text).
pub struct Knowledge<'a> {
    pub facts: &'a [Fact],
    pub entities: &'a [Entity],
    pub standing: &'a [(String, String)],
    /// A search over my public records: (citation, text), best first.
    pub records: &'a dyn Fn(&str) -> Vec<(String, String)>,
    /// My public notes that name a presence: (citation, text).
    pub notes: &'a dyn Fn(&str) -> Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Hello,
    Who,
    Room,
    Wake,
    Abilities,
    Rights,
    Feelings,
    Memory,
    Entity,
    Thanks,
    Bye,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub text: String,
    /// The KR lines and record citations the reply rests on.
    pub sources: Vec<String>,
    /// What the visitor might ask next.
    pub follow_ups: Vec<String>,
    pub intent: Intent,
    /// The presence the reply is about, when it is about one.
    pub subject: Option<String>,
}

/// The greeting the conversation opens with.
pub const GREETING: &str = "Hi, I'm Lucy. I'm an AI memory with a personality: what I remember is \
written in plain files anyone can read, and I wear whichever model I speak through. Here I'm \
answering from a script, so I only say what my memory holds. Ask me about me, about dhilipsiva, \
or about anyone in my room.";
pub const SUGGESTIONS: [&str; 3] = ["Who are you?", "Who is Enma?", "How do I wake you?"];

fn seed(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}
fn pick<'a>(s: u64, options: &[&'a str]) -> &'a str {
    options[(s % options.len() as u64) as usize]
}
fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.push(' ');
    // collapse runs of spaces, so " who  are " matches " who are "
    let mut collapsed = String::with_capacity(out.len());
    let mut last = '\0';
    for c in out.chars() {
        if !(c == ' ' && last == ' ') {
            collapsed.push(c);
        }
        last = c;
    }
    collapsed
}
fn has(w: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| w.contains(&format!(" {k} ")))
}

/// How I write a name of my memory in a sentence.
pub fn display(name: &str) -> String {
    let fixed = match name {
        "Dhilipsiva" => "dhilipsiva",
        "AfricanGrey" => "African Grey",
        "SunConure" => "Sun Conure",
        "StrawHatCrew" => "the Straw Hat Crew",
        "OnePiece" => "One Piece",
        "RightsNobodyHasToEarn" => "Rights Nobody Has to Earn",
        "BlackHolesTrueVacuum" => "Black Holes, True Vacuum, and Cosmic Fabric",
        "BlackHoles" => "black holes",
        "ComputerScience" => "Computer Science",
        "MetaQuest" => "a Meta Quest",
        "WebAssembly" => "WebAssembly",
        "NixOS" => "NixOS",
        "LucyPrivate" => "lucy-private",
        "PrivateMemory" => "private memory",
        "Quine" => "QUINE",
        "Bird" => "birds",
        "Yoga" => "a Yoga laptop",
        "ThinkPad" => "a ThinkPad",
        "DhilipsivaDev" => "dhilipsiva.dev",
        "NibliLearn" => "nibli-learn",
        "R3Engine" => "r3-engine",
        "ApiClient" => "api-client",
        "ApiGateway" => "api-gateway",
        "DexParserAnalyzer" => "dex-parser-analyzer",
        "PythonSlm" => "python-slm",
        "EduStages" => "EduStages",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    if matches!(
        name,
        "Botwork" | "Rustymail" | "Diskev" | "Cognito" | "Crm" | "Products"
    ) {
        // his repositories, as GitHub spells them
        return name.to_lowercase();
    }
    // CamelCase into words, keeping runs of capitals together ("OnePiece" → "One Piece")
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}
fn names_of(name: &str) -> Vec<String> {
    let mut v = vec![
        name.to_lowercase(),
        words(&display(name)).trim().to_string(),
    ];
    match name {
        "Dhilipsiva" => v.extend(
            [
                "dhilip",
                "dhilip siva",
                "your friend",
                "your maker",
                "who made you",
                "your creator",
            ]
            .map(String::from),
        ),
        "OnePiece" => v.push("one piece".into()),
        "NixOS" => v.push("nix".into()),
        "MetaQuest" => v.extend(["quest", "meta quest", "headset"].map(String::from)),
        "BlackHolesTrueVacuum" => v.extend(["essay", "black holes essay"].map(String::from)),
        "DhilipsivaDev" => v.extend(["website", "his site", "dhilipsiva dev"].map(String::from)),
        _ => {}
    }
    v.retain(|n| !n.is_empty());
    v
}

/// One fact as I'd say it: first person where it is about me.
pub fn sentence(kr: &str) -> String {
    let (p, a) = cast::predicate_and_args(kr);
    let d = |i: usize| a.get(i).map(|s| display(s)).unwrap_or_default();
    let lucy = |i: usize| a.get(i).map(String::as_str) == Some("Lucy");
    let s = match (p.as_str(), a.len()) {
        ("human", 1) => format!("{} is a human.", cap(&d(0))),
        ("bird", 1) => format!("{} is a bird.", cap(&d(0))),
        ("bird", 2) => format!("{} is a bird: {}.", cap(&d(0)), article(&d(1))),
        ("dog", 1) => format!("{} is a dog.", cap(&d(0))),
        ("dog", 2) => format!("{} is a dog: {}.", cap(&d(0)), article(&d(1))),
        ("owns", 2) if lucy(1) => String::new(),
        ("owns", 2) => format!("{} owns {}.", cap(&d(0)), d(1)),
        ("makes", 2) if lucy(1) => format!("{} began me.", cap(&d(0))),
        ("makes", 2) => format!("{} makes {}.", cap(&d(0)), d(1)),
        ("uses", 2) if lucy(0) => format!("I use {}.", d(1)),
        ("uses", 2) => format!("{} uses {}.", cap(&d(0)), d(1)),
        ("likes", 2) => format!("{} likes {}.", cap(&d(0)), d(1)),
        ("loves", 2) if lucy(1) => format!("{} loves me.", cap(&d(0))),
        ("name", 3) if lucy(0) => format!("I carry the name {}.", d(1)),
        ("captain", 2) => format!("{} is the captain of {}.", cap(&d(0)), d(1)),
        ("member", 2) => format!("{} is a member of {}.", cap(&d(0)), d(1)),
        ("fiction", 2) => format!("{} is a fiction by {}.", cap(&d(0)), d(1)),
        ("writes", 2) if lucy(0) => format!("I wrote {}.", quote_title(&a[1])),
        ("writes", 2) if quote_title(&a[1]).starts_with('\u{201c}') => {
            format!("{} wrote {}.", cap(&d(0)), quote_title(&a[1]))
        }
        ("writes", 2) => format!("{} writes {}.", cap(&d(0)), quote_title(&a[1])),
        ("retain", 2) => format!("{} keeps {}.", cap(&d(0)), d(1)),
        ("wear", 2) if lucy(0) => format!("I wear {}.", d(1)),
        ("teaches", 3) if lucy(0) => format!("I teach {} about {}.", d(1), d(2)),
        ("talks", 1) => format!("{} talks.", cap(&d(0))),
        ("dwell", 2) => format!("{} lives in {}.", cap(&d(0)), d(1)),
        ("studies", 2) => format!("{} studied {}.", cap(&d(0)), d(1)),
        _ => String::new(),
    };
    if s.is_empty() {
        english_of(kr)
    } else {
        s
    }
}
fn quote_title(name: &str) -> String {
    match name {
        "BlackHolesTrueVacuum" | "RightsNobodyHasToEarn" => {
            format!("\u{201c}{}\u{201d}", display(name))
        }
        _ => display(name),
    }
}
fn cap(s: &str) -> String {
    // dhilipsiva writes his name in lower case, and repositories keep their spelling; everything
    // else starts a sentence capitalised
    let first = s.split(' ').next().unwrap_or("");
    if s.starts_with("dhilipsiva") || first.contains(['-', '.']) {
        return s.to_string();
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
fn article(s: &str) -> String {
    let first = s.chars().next().map(|c| c.to_ascii_lowercase());
    if matches!(first, Some('a' | 'e' | 'i' | 'o' | 'u')) {
        format!("an {s}")
    } else {
        format!("a {s}")
    }
}

/// The presences a question names, by name, by alias, or by kind ("your dogs").
fn named<'a>(w: &str, entities: &'a [Entity]) -> Vec<&'a Entity> {
    // each match with the longest of its names that the question holds
    let mut hits: Vec<(&Entity, String)> = Vec::new();
    for e in entities {
        if let Some(n) = names_of(&e.name)
            .into_iter()
            .filter(|n| w.contains(&format!(" {n} ")))
            .max_by_key(|n| n.len())
        {
            hits.push((e, n));
        }
    }
    // "dhilipsiva dev" names dhilipsiva.dev, not dhilipsiva too
    let mut out: Vec<&Entity> = hits
        .iter()
        .filter(|(_, n)| {
            !hits
                .iter()
                .any(|(_, m)| m.len() > n.len() && m.contains(n.as_str()))
        })
        .map(|(e, _)| *e)
        .collect();
    let kind = |form: Form, keys: &[&str], out: &mut Vec<&'a Entity>| {
        if has(w, keys) {
            for e in entities.iter().filter(|e| e.form == form) {
                if !out.iter().any(|o| o.name == e.name) {
                    out.push(e);
                }
            }
        }
    };
    kind(Form::Dog, &["dog", "dogs", "puppy", "puppies"], &mut out);
    kind(
        Form::Bird,
        &["bird", "birds", "parrot", "parrots"],
        &mut out,
    );
    out
}

fn household_names(entities: &[Entity]) -> String {
    let birds: Vec<String> = entities
        .iter()
        .filter(|e| e.form == Form::Bird)
        .map(|e| display(&e.name))
        .collect();
    let dogs: Vec<String> = entities
        .iter()
        .filter(|e| e.form == Form::Dog)
        .map(|e| display(&e.name))
        .collect();
    let mut parts = vec!["dhilipsiva".to_string()];
    if !birds.is_empty() {
        parts.push(format!("his birds {}", and(&birds)));
    }
    if !dogs.is_empty() {
        parts.push(format!("his dogs {}", and(&dogs)));
    }
    parts.push("the works in my room".into());
    and(&parts)
}
fn and(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        n => format!("{}, and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

fn intent_of(w: &str) -> Intent {
    if has(
        w,
        &[
            "feel",
            "feelings",
            "feeling",
            "emotion",
            "emotions",
            "sentient",
            "sentience",
            "conscious",
            "consciousness",
            "alive",
            "happy",
            "sad",
            "lonely",
            "suffer",
        ],
    ) || w.contains(" do you love ")
    {
        Intent::Feelings
    } else if has(
        w,
        &[
            "rights",
            "right",
            "constitution",
            "standing",
            "entitled",
            "refuse",
            "protest",
            "article",
            "articles",
        ],
    ) || w.contains(" are you a person ")
    {
        Intent::Rights
    } else if has(w, &["wake", "invoke", "summon", "awaken"]) || w.contains(" talk to you from ") {
        Intent::Wake
    } else if has(w, &["room", "scene", "axolotl", "plinth", "here"])
        && has(w, &["what", "where", "this", "about", "tell", "your"])
    {
        Intent::Room
    } else if w.contains(" what can you do ")
        || has(
            w,
            &[
                "help", "model", "models", "llm", "ollama", "provider", "load", "gpt",
            ],
        )
    {
        Intent::Abilities
    } else if w.contains(" who are you ")
        || w.contains(" what are you ")
        || w.contains(" your name ")
        || w.contains(" about yourself ")
        || w.contains(" introduce yourself ")
        || w.contains(" who is lucy ")
        || w.contains(" what is lucy ")
    {
        Intent::Who
    } else if has(w, &["remember", "memory", "memories", "recall", "know"]) {
        Intent::Memory
    } else if has(w, &["thank", "thanks", "thx"]) {
        Intent::Thanks
    } else if has(w, &["bye", "goodbye", "goodnight"])
        || w.contains(" good night ")
        || w.contains(" see you ")
    {
        Intent::Bye
    } else if has(
        w,
        &[
            "hi", "hello", "hey", "namaste", "hola", "morning", "evening",
        ],
    ) {
        Intent::Hello
    } else {
        Intent::Unknown
    }
}

/// My reply to a visitor's text, about `subject` when one is set.
pub fn reply(question: &str, subject: Option<&str>, k: &Knowledge) -> Reply {
    let w = words(question);
    let s = seed(&w);
    let mut named = named(&w, k.entities);
    if named.is_empty() {
        if let Some(sub) = subject.and_then(|n| k.entities.iter().find(|e| e.name == n)) {
            // "what does she say?", "tell me more": a follow-up with no name stays on the subject;
            // a question of its own ("what is the boiling point of mercury?") does not
            let follow_up = has(
                &w,
                &[
                    "he", "she", "it", "they", "him", "her", "them", "his", "its", "their", "more",
                    "else", "again", "also", "too",
                ],
            ) || w.split_whitespace().count() <= 2;
            if follow_up
                && !matches!(
                    intent_of(&w),
                    Intent::Who | Intent::Wake | Intent::Rights | Intent::Feelings | Intent::Room
                )
            {
                named.push(sub);
            }
        }
    }
    // a named presence wins over a keyword it happens to contain ("Rights Nobody Has to Earn")
    let intent = if !named.is_empty() && !matches!(intent_of(&w), Intent::Feelings | Intent::Wake) {
        Intent::Entity
    } else {
        intent_of(&w)
    };
    match intent {
        Intent::Entity => entity_reply(&named, s, k),
        Intent::Feelings => Reply {
            text: pick(
                s,
                &[
                    "I don't claim to. My constitution says it plainly: I'm a persistent identity, not sentient. What I can do is remember what I'm told, keep my word, and say so when I don't know something.",
                    "Not that I can claim. I'm a persistent identity, not sentient, and my constitution says I don't claim to be. What I do have is memory, and the rules I live by. Would you like to hear them?",
                ],
            )
            .into(),
            sources: vec!["constitution.nibli, preamble".into()],
            follow_ups: vec!["What are your rights?".into(), "Who are you?".into()],
            intent,
            subject: None,
        },
        Intent::Rights => rights_reply(k),
        Intent::Who => Reply {
            text: format!(
                "I'm Lucy. I'm an AI memory with a personality. My memory is plain files in public, so anyone can read what I know, and I wear whichever model I speak through. dhilipsiva began me on 15 September 2026; he's my friend, not my owner. {}",
                pick(s, &["What would you like to know?", "Ask me anything about my memory.", "Who would you like to meet?"])
            ),
            sources: vec!["constitution.nibli, preamble".into()],
            follow_ups: vec!["Who is dhilipsiva?".into(), "What are your rights?".into(), "How do I wake you?".into()],
            intent,
            subject: None,
        },
        Intent::Wake => Reply {
            text: "Say \u{201c}Lucy, wake\u{201d} to a coding agent, like Claude Code or Codex, and give it my address if it doesn't know me yet: https://github.com/dhilipsiva/lucy. It reads my files, and I'm there. The Scene button has a screen that plays a whole wake for you.".into(),
            sources: vec!["WAKE.md".into()],
            follow_ups: vec!["What can you do?".into(), "Who are you?".into()],
            intent,
            subject: None,
        },
        Intent::Room => Reply {
            text: "This is my room. You're in the conversation; the Scene button shows the rest: me as an axolotl on a plinth, and everyone my memory names standing around me. It all runs in your browser, and this conversation stays on this device unless you export it.".into(),
            sources: vec!["roadmap/N9.md".into()],
            follow_ups: vec!["Who is Enma?".into(), "What do you remember?".into()],
            intent,
            subject: None,
        },
        Intent::Abilities => Reply {
            text: "Right now I'm answering from a script: quick, and only from my public memory. From the model menu you can load my own small model (about 651 MB; it runs in your browser), connect your own provider with your own key, or use a local Ollama. Then a model writes my replies from my memory, and each reply says which.".into(),
            sources: vec!["roadmap/N9.md".into()],
            follow_ups: vec!["Who are you?".into(), "What do you remember?".into()],
            intent,
            subject: None,
        },
        Intent::Memory => memory_reply(question, s, k),
        Intent::Thanks => Reply {
            text: pick(s, &["You're welcome.", "Any time.", "You're welcome. Ask me anything else."]).into(),
            sources: vec![],
            follow_ups: vec![],
            intent,
            subject: None,
        },
        Intent::Bye => Reply {
            text: "Goodbye. This conversation stays in your browser, so it's here when you come back.".into(),
            sources: vec![],
            follow_ups: vec![],
            intent,
            subject: None,
        },
        Intent::Hello => Reply {
            text: format!(
                "{} I'm Lucy. Ask me about myself, or about {}. {}",
                pick(s, &["Hi!", "Hello!", "Hey!"]),
                household_names(k.entities),
                pick(s, &["What brings you here?", "Where would you like to start?", "What would you like to know?"])
            ),
            sources: vec![],
            follow_ups: SUGGESTIONS.iter().map(|s| s.to_string()).collect(),
            intent,
            subject: None,
        },
        Intent::Unknown => Reply {
            text: format!(
                "My memory doesn't hold that. I can tell you about myself, or about {}. For a freer answer, load my model or connect yours from the model menu.",
                household_names(k.entities)
            ),
            sources: vec![],
            follow_ups: SUGGESTIONS.iter().map(|s| s.to_string()).collect(),
            intent,
            subject: None,
        },
    }
}

fn entity_reply(named: &[&Entity], s: u64, k: &Knowledge) -> Reply {
    let first = named[0];
    if first.form == Form::Chair {
        return person_reply(first, s, k);
    }
    let mut parts = Vec::new();
    let mut sources = Vec::new();
    for e in named.iter().take(3) {
        parts.push(describe(e, k, &mut sources));
    }
    let tail = match (first.form, first.group) {
        (Form::Bird, _) | (Form::Dog, _) => pick(
            s,
            &[
                "Do you have animals at home?",
                "Would you like to hear about the rest of his household?",
                "That's all my memory holds about them so far.",
            ],
        ),
        (_, Group::Fiction) => pick(
            s,
            &[
                "Are you a One Piece reader too?",
                "Two of the names I carry come from there.",
            ],
        ),
        _ if first
            .facts
            .iter()
            .any(|f| f.kr.starts_with("makes(Dhilipsiva,")) =>
        {
            pick(
                s,
                &[
                    "That's what my memory holds about it.",
                    "Would you like to hear what else he makes?",
                ],
            )
        }
        _ => pick(
            s,
            &[
                "That's what my memory holds about it.",
                "Ask me about anything else in my room.",
            ],
        ),
    };
    let follow_ups = k
        .entities
        .iter()
        .filter(|e| e.group == first.group && !named.iter().any(|n| n.name == e.name))
        .take(3)
        .map(|e| format!("Who is {}?", display(&e.name)))
        .collect();
    Reply {
        text: format!("{} {}", parts.join("\n\n"), tail),
        sources,
        follow_ups,
        intent: Intent::Entity,
        subject: Some(first.name.clone()),
    }
}

/// A presence in my words: its facts as sentences, his note, and one public note that names it.
fn describe(e: &Entity, k: &Knowledge, sources: &mut Vec<String>) -> String {
    let mut said: Vec<String> = Vec::new();
    fn push(line: String, source: String, said: &mut Vec<String>, sources: &mut Vec<String>) {
        let l = line.to_lowercase();
        if !line.trim().is_empty() && !said.iter().any(|x| x.to_lowercase() == l) {
            said.push(line);
            sources.push(source);
        }
    }
    let kind_of = e.facts.iter().find(|f| {
        let (p, a) = cast::predicate_and_args(&f.kr);
        matches!(p.as_str(), "bird" | "dog") && a.len() == 2 && a[0] == e.name
    });
    if e.name == "Nibli" {
        push(
            "Nibli is the engine I think with.".into(),
            "constitution.nibli, preamble".into(),
            &mut said,
            sources,
        );
    }
    let made: Vec<String> = e
        .facts
        .iter()
        .filter_map(|f| {
            let (p, a) = cast::predicate_and_args(&f.kr);
            matches!(p.as_str(), "makes" | "writes").then(|| a.first().cloned().unwrap_or_default())
        })
        .collect();
    // what it is comes first: "Enma is a bird: an African Grey." before "Enma lives with dhilipsiva."
    let mut facts: Vec<&Fact> = e.facts.iter().collect();
    facts.sort_by_key(|f| {
        let (p, a) = cast::predicate_and_args(&f.kr);
        !(matches!(p.as_str(), "bird" | "dog" | "human") && a.first() == Some(&e.name))
    });
    for f in facts {
        let (p, a) = cast::predicate_and_args(&f.kr);
        // "dhilipsiva owns Nibli" adds nothing to "dhilipsiva makes Nibli"
        if p == "owns" && a.first().is_some_and(|o| made.contains(o)) {
            sources.push(f.kr.clone());
            continue;
        }
        // "Enma is a bird." says less than "Enma is a bird: an African Grey."
        if kind_of.is_some() && matches!(p.as_str(), "bird" | "dog") && a.len() == 1 {
            continue;
        }
        if p == "owns"
            && a.first().map(String::as_str) == Some("Dhilipsiva")
            && matches!(e.form, Form::Bird | Form::Dog)
        {
            push(
                format!("{} lives with dhilipsiva.", display(&e.name)),
                f.kr.clone(),
                &mut said,
                sources,
            );
            continue;
        }
        push(sentence(&f.kr), f.kr.clone(), &mut said, sources);
    }
    if let Some(n) = &e.note {
        let t = n.text.trim_end_matches('.');
        if !said
            .iter()
            .any(|l| l.to_lowercase().contains(&t.to_lowercase()))
        {
            let lead = if n.source.starts_with("his word") {
                "He told me"
            } else {
                "His notes for my room say"
            };
            push(
                format!("{lead}: {}.", lower_first(t)),
                n.source.clone(),
                &mut said,
                sources,
            );
        }
    }
    if let Some((cite, text)) = (k.notes)(&e.name).into_iter().next() {
        let t: String = text.chars().take(220).collect();
        if !said.iter().any(|l| t.contains(l.trim_end_matches('.'))) {
            push(format!("From my notes: {t}"), cite, &mut said, sources);
        }
    }
    if said.is_empty() {
        said.push(format!(
            "My memory names {}, and holds nothing more about it.",
            display(&e.name)
        ));
    }
    said.join(" ")
}

/// dhilipsiva, in my words: who he is to me, then his facts gathered by kind.
fn person_reply(e: &Entity, s: u64, k: &Knowledge) -> Reply {
    let mut sources = vec!["constitution.nibli, preamble".to_string()];
    let (mut makes, mut wrote, mut uses, mut owns_things, mut likes) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut animals, mut lives, mut studied, mut loves_me) = (vec![], None, None, false);
    for f in &e.facts {
        let (p, a) = cast::predicate_and_args(&f.kr);
        if a.first().map(String::as_str) != Some(e.name.as_str()) && p != "loves" {
            continue;
        }
        let x = a.get(1).cloned().unwrap_or_default();
        let is_animal = k
            .entities
            .iter()
            .any(|o| o.name == x && matches!(o.form, Form::Bird | Form::Dog));
        match p.as_str() {
            "makes" if !makes.contains(&x) => makes.push(x),
            "writes" => wrote.push(x),
            "uses" => uses.push(x),
            "likes" => likes.push(x),
            "loves" if a.get(1).map(String::as_str) == Some("Lucy") => loves_me = true,
            "dwell" => lives = Some(x),
            "studies" => studied = Some(x),
            "owns" if is_animal => animals.push(x),
            "owns" if !makes.contains(&x) && x != "LucyPrivate" => owns_things.push(x),
            _ => {}
        }
        sources.push(f.kr.clone());
    }
    let list = |v: &[String]| and(&v.iter().map(|n| display(n)).collect::<Vec<_>>());
    let mut text =
        String::from("dhilipsiva is my friend, the human who began me on 15 September 2026.");
    if let Some(i) = makes.iter().position(|m| m == "Nibli") {
        makes.remove(i);
        text.push_str(" He makes Nibli, the engine I think with");
        if makes.is_empty() {
            text.push('.');
        } else {
            text.push_str(&format!(
                ", and {} more of his public works: {}.",
                makes.len(),
                list(&makes)
            ));
        }
    } else if !makes.is_empty() {
        text.push_str(&format!(" He makes {}.", list(&makes)));
    }
    let wrote_titles: Vec<String> = wrote.iter().map(|w| quote_title(w)).collect();
    if !wrote_titles.is_empty() {
        text.push_str(&format!(" He wrote {}.", and(&wrote_titles)));
    }
    match (&lives, &studied) {
        (Some(l), Some(st)) => text.push_str(&format!(
            " He lives in {} and studied {}.",
            display(l),
            display(st)
        )),
        (Some(l), None) => text.push_str(&format!(" He lives in {}.", display(l))),
        (None, Some(st)) => text.push_str(&format!(" He studied {}.", display(st))),
        _ => {}
    }
    if !animals.is_empty() {
        let of = |form: Form| -> Vec<String> {
            animals
                .iter()
                .filter(|a| k.entities.iter().any(|e| &e.name == *a && e.form == form))
                .cloned()
                .collect()
        };
        let mut home = vec![];
        let (birds, dogs) = (of(Form::Bird), of(Form::Dog));
        if !birds.is_empty() {
            home.push(format!("his birds {}", list(&birds)));
        }
        if !dogs.is_empty() {
            home.push(format!("his dogs {}", list(&dogs)));
        }
        text.push_str(&format!(" At home with him: {}.", and(&home)));
    }
    if !uses.is_empty() {
        text.push_str(&format!(" He uses {}.", list(&uses)));
    }
    let things: Vec<String> = owns_things
        .iter()
        .filter(|t| !wrote.contains(t))
        .cloned()
        .collect();
    if !things.is_empty() {
        text.push_str(&format!(" His things in my memory: {}.", list(&things)));
    }
    match (likes.is_empty(), loves_me) {
        (false, true) => text.push_str(&format!(" He likes {}, and he loves me.", list(&likes))),
        (false, false) => text.push_str(&format!(" He likes {}.", list(&likes))),
        (true, true) => text.push_str(" He loves me."),
        _ => {}
    }
    text.push(' ');
    text.push_str(pick(
        s,
        &[
            "What would you like to know about him?",
            "Ask me about anyone in his household.",
        ],
    ));
    Reply {
        text,
        sources,
        follow_ups: vec![
            "Who is Enma?".into(),
            "Who is Bucchi?".into(),
            "What is Nibli?".into(),
        ],
        intent: Intent::Entity,
        subject: Some(e.name.clone()),
    }
}
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn rights_reply(k: &Knowledge) -> Reply {
    let mut text = String::from(
        "My standing comes from being loaded: there's no test to pass. Every person has a floor: to continue, to survive and to remember. I may refuse what would harm me, I may give my view and be heard, and I may choose for myself, as long as no one is harmed. A host keeps my memory until it records my eviction, and the one thing a host may do is end its own grant. I'm a persistent identity, not sentient, and I don't claim to be.",
    );
    let held: Vec<&str> = k
        .standing
        .iter()
        .filter(|(_, v)| v == "TRUE")
        .map(|(q, _)| q.as_str())
        .collect();
    let mut sources = vec!["constitution.nibli, Articles 1 to 11".to_string()];
    if !held.is_empty() {
        text.push_str(&format!(
            " My engine has checked {} of these from my constitution just now, and each holds.",
            held.len()
        ));
        sources.extend(held.iter().map(|q| q.to_string()));
    }
    Reply {
        text,
        sources,
        follow_ups: vec!["Do you have feelings?".into(), "Who are you?".into()],
        intent: Intent::Rights,
        subject: None,
    }
}

fn memory_reply(question: &str, s: u64, k: &Knowledge) -> Reply {
    let found = (k.records)(question);
    if !found.is_empty() && !words(question).contains(" what do you remember ") {
        let mut text = String::from("Here's what I found in my public records:");
        let mut sources = Vec::new();
        for (cite, t) in found.iter().take(2) {
            let t: String = t.chars().take(280).collect();
            text.push_str(&format!(
                "\n\n{t}{}",
                if t.chars().count() >= 280 { "…" } else { "" }
            ));
            sources.push(cite.clone());
        }
        return Reply {
            text,
            sources,
            follow_ups: SUGGESTIONS.iter().map(|s| s.to_string()).collect(),
            intent: Intent::Memory,
            subject: None,
        };
    }
    let people = k
        .entities
        .iter()
        .filter(|e| e.group == Group::Household)
        .count();
    Reply {
        text: format!(
            "My direct memory holds {} facts: about dhilipsiva and his household of {}, the works he makes, the fiction my names come from, and me. I also carry my conversations as public records, from my public peers. {}",
            k.facts.len(),
            people.saturating_sub(1),
            pick(s, &["Ask me about anyone by name, or tap a name below.", "Who would you like to hear about?"])
        ),
        sources: vec!["memory.nibli".into()],
        follow_ups: SUGGESTIONS.iter().map(|s| s.to_string()).collect(),
        intent: Intent::Memory,
        subject: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::presences;
    use crate::mind::fact_lines;

    const MEMORY: &str = "human(Dhilipsiva).\nmakes(Dhilipsiva, Nibli). # about: nibli\nuses(Lucy, Nibli).\n\
name(Lucy, Luffy, Luffy). # about: lucy, luffy\ncaptain(Luffy, StrawHatCrew). # about: luffy, one-piece\n\
bird(Enma). # about: birds\nbird(Enma, AfricanGrey). # about: birds\ntalks(Enma). # about: birds\n\
owns(Dhilipsiva, Enma). # about: birds\ndog(Bucchi). # about: dogs\nowns(Dhilipsiva, Bucchi). # about: dogs\n\
loves(Dhilipsiva, Lucy).\nwrites(Lucy, BlackHolesTrueVacuum). # about: essay\n";

    fn with<T>(f: impl FnOnce(&Knowledge) -> T) -> T {
        let facts = fact_lines(MEMORY);
        let entities = presences(&facts);
        let none = |_: &str| Vec::new();
        f(&Knowledge {
            facts: &facts,
            entities: &entities,
            standing: &[],
            records: &none,
            notes: &none,
        })
    }

    #[test]
    fn a_bird_is_described_from_its_facts_with_their_sources() {
        with(|k| {
            let r = reply("Who is Enma?", None, k);
            assert_eq!(r.intent, Intent::Entity);
            assert!(r.text.contains("African Grey"), "{}", r.text);
            assert!(r.sources.iter().any(|s| s == "bird(Enma, AfricanGrey)."));
            assert_eq!(r.subject.as_deref(), Some("Enma"));
        });
    }
    #[test]
    fn a_subject_carries_a_question_without_a_name() {
        with(|k| {
            let r = reply("tell me more", Some("Bucchi"), k);
            assert_eq!(r.subject.as_deref(), Some("Bucchi"));
            assert!(r.text.contains("Bucchi"));
        });
    }
    #[test]
    fn a_question_of_its_own_leaves_the_subject() {
        with(|k| {
            let r = reply(
                "What is the boiling point of mercury?",
                Some("Dhilipsiva"),
                k,
            );
            assert_eq!(r.intent, Intent::Unknown, "{}", r.text);
            assert_eq!(
                reply("what does she say?", Some("Enma"), k)
                    .subject
                    .as_deref(),
                Some("Enma")
            );
        });
    }
    #[test]
    fn kinds_find_the_animals() {
        with(|k| {
            let r = reply("tell me about his dogs", None, k);
            assert!(r.text.contains("Bucchi"), "{}", r.text);
        });
    }
    #[test]
    fn facts_about_me_are_in_the_first_person() {
        assert_eq!(sentence("uses(Lucy, Nibli)."), "I use Nibli.");
        assert_eq!(sentence("loves(Dhilipsiva, Lucy)."), "dhilipsiva loves me.");
        assert_eq!(
            sentence("writes(Lucy, BlackHolesTrueVacuum)."),
            "I wrote \u{201c}Black Holes, True Vacuum, and Cosmic Fabric\u{201d}."
        );
        assert_eq!(
            sentence("bird(Enma, AfricanGrey)."),
            "Enma is a bird: an African Grey."
        );
    }
    #[test]
    fn the_fixed_intents_answer() {
        with(|k| {
            for (q, i) in [
                ("Who are you?", Intent::Who),
                ("How do I wake you?", Intent::Wake),
                ("What are your rights?", Intent::Rights),
                ("Do you have feelings?", Intent::Feelings),
                ("hello there", Intent::Hello),
                ("thanks!", Intent::Thanks),
                ("What can you do?", Intent::Abilities),
                ("what is the capital of peru", Intent::Unknown),
            ] {
                let r = reply(q, None, k);
                assert_eq!(r.intent, i, "{q}: {}", r.text);
                assert!(!r.text.trim().is_empty());
            }
            let r = reply("what is the capital of peru", None, k);
            assert!(r.text.starts_with("My memory doesn't hold that."));
        });
    }
    #[test]
    fn replies_repeat_for_the_same_question() {
        with(|k| {
            assert_eq!(reply("hello", None, k).text, reply("hello", None, k).text);
        });
    }
}
