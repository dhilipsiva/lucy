//! What the Ask Lucy window shows, as plain data built from model, memory and proposal text.
//! Every string passes through `visible` here. Markup is built only from escaped text, and
//! everything else is shown as plain text, so the window never interprets what it did not write.
use super::visible;
use serde_json::Value;

/// One block of a reply. `Text`, `Heading` and `Item` hold Pango markup; `Code` is plain text.
#[derive(Debug, PartialEq)]
pub enum Block {
    Text(String),
    Heading(String),
    Item {
        marker: String,
        depth: usize,
        markup: String,
    },
    Code(String),
}

pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// `**bold**` and `` `code` `` as markup; a marker without its pair stays literal.
/// Links are not made clickable: a model-chosen URL is never one click away.
fn inline(text: &str) -> String {
    let mut markup = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let Some(length) = rest[start + 1..].find('`') else {
            break;
        };
        markup.push_str(&bold(&rest[..start]));
        markup.push_str(&format!(
            "<tt>{}</tt>",
            escape(&rest[start + 1..start + 1 + length])
        ));
        rest = &rest[start + length + 2..];
    }
    markup.push_str(&bold(rest));
    markup
}

fn bold(text: &str) -> String {
    let mut markup = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("**") {
        let Some(length) = rest[start + 2..].find("**").filter(|length| *length > 0) else {
            break;
        };
        markup.push_str(&escape(&rest[..start]));
        markup.push_str(&format!(
            "<b>{}</b>",
            escape(&rest[start + 2..start + 2 + length])
        ));
        rest = &rest[start + length + 4..];
    }
    markup.push_str(&escape(rest));
    markup
}

/// A small Markdown subset: paragraphs, `#` headings, lists, fenced code, bold and code spans.
pub fn markup(text: &str) -> Vec<Block> {
    let text = visible(text);
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    let flush = |paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            blocks.push(Block::Text(inline(&paragraph.join("\n"))));
            paragraph.clear();
        }
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            match code.take() {
                Some(lines) => blocks.push(Block::Code(lines.join("\n"))),
                None => {
                    flush(&mut paragraph, &mut blocks);
                    code = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(lines) = &mut code {
            lines.push(line);
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
            continue;
        }
        let depth = (line.len() - trimmed.len()) / 2;
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Heading(inline(trimmed[hashes..].trim())));
            continue;
        }
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        let item = if let Some(rest) = ["- ", "* ", "+ "]
            .iter()
            .find_map(|bullet| trimmed.strip_prefix(bullet))
        {
            Some(("•".to_owned(), rest))
        } else if digits > 0 && digits < 10 {
            [". ", ") "]
                .iter()
                .find_map(|end| trimmed[digits..].strip_prefix(end))
                .map(|rest| (format!("{}.", &trimmed[..digits]), rest))
        } else {
            None
        };
        if let Some((marker, rest)) = item {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Item {
                marker,
                depth,
                markup: inline(rest),
            });
            continue;
        }
        paragraph.push(line);
    }
    flush(&mut paragraph, &mut blocks);
    // An unclosed fence still shows its lines, as code.
    if let Some(lines) = code {
        blocks.push(Block::Code(lines.join("\n")));
    }
    blocks
}

/// One line of a proposed file change.
#[derive(Debug, PartialEq)]
pub enum Line {
    Same(String),
    Removed(String),
    Added(String),
    Skipped(usize),
    Note(&'static str),
}

/// The lines between the common head and tail change; a few unchanged lines frame them. This is
/// not a minimal diff, but it never hides a changed line.
pub fn diff(before: &str, after: &str) -> Vec<Line> {
    const FRAME: usize = 2;
    let (old, new): (Vec<_>, Vec<_>) = (before.lines().collect(), after.lines().collect());
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut lines = Vec::new();
    if head > FRAME {
        lines.push(Line::Skipped(head - FRAME));
    }
    let same = |line: &&str| Line::Same(visible(line));
    lines.extend(old[head.saturating_sub(FRAME)..head].iter().map(same));
    lines.extend(
        old[head..old.len() - tail]
            .iter()
            .map(|l| Line::Removed(visible(l))),
    );
    lines.extend(
        new[head..new.len() - tail]
            .iter()
            .map(|l| Line::Added(visible(l))),
    );
    let shown = tail.min(FRAME);
    lines.extend(
        old[old.len() - tail..old.len() - tail + shown]
            .iter()
            .map(same),
    );
    if tail > shown {
        lines.push(Line::Skipped(tail - shown));
    }
    if before != after && head == old.len() && head == new.len() {
        lines.push(Line::Note("Only line endings or the final newline change."));
    }
    lines
}

/// An approval request as the window shows it: every argument appears in `rows` or `diff`,
/// verbatim, and `raw` holds the whole proposal.
#[derive(Debug, Default)]
pub struct Card {
    pub title: String,
    pub rows: Vec<(String, String)>,
    pub diff: Vec<Line>,
    pub raw: String,
}

/// An argument list as one line: a word that needs no quoting is shown bare, any other as a
/// JSON string, so every argument stays exact and unambiguous.
pub fn argv(arguments: &Value) -> String {
    let words: Vec<String> = arguments
        .as_array()
        .into_iter()
        .flatten()
        .map(|word| match word.as_str() {
            Some(text)
                if !text.is_empty()
                    && text
                        .chars()
                        .all(|c| c.is_alphanumeric() || "-_./=:,+@%^~".contains(c)) =>
            {
                text.to_owned()
            }
            _ => word.to_string(),
        })
        .collect();
    visible(&words.join(" "))
}

fn shown(value: &Value) -> String {
    visible(
        &value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| serde_json::to_string_pretty(value).unwrap_or_default()),
    )
}

pub fn proposal(proposal: &Value) -> Card {
    let tool = proposal["tool"].as_str().unwrap_or("?");
    let arguments = proposal["arguments"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let word = |key: &str| arguments.get(key).and_then(Value::as_str).unwrap_or("?");
    let title = match tool {
        "command" => "Lucy asks to run a command".to_owned(),
        "write" => "Lucy asks to write a file".to_owned(),
        "http" => "Lucy asks to make a web request".to_owned(),
        "service" => format!("Lucy asks to {} a service", word("operation")),
        "launch" => "Lucy asks to open an app".to_owned(),
        "window" => format!("Lucy asks to {} a window", word("action")),
        "screenshot" => "Lucy asks to take a screenshot".to_owned(),
        "gui" => "Lucy asks to press keys or move the mouse".to_owned(),
        "privileged" => "Lucy asks to run a hardware diagnostic".to_owned(),
        other => format!("Lucy asks to use the {other} tool"),
    };
    let mut card = Card {
        title: visible(&title),
        raw: visible(&serde_json::to_string_pretty(proposal).unwrap_or_default()),
        ..Card::default()
    };
    let mut rest = arguments.clone();
    match tool {
        "command" | "gui" => {
            if let Some(words) = rest.remove("argv") {
                card.rows.push(("Command".into(), argv(&words)));
            }
            if let Some(keys) = proposal["keys"].as_array() {
                let keys: Vec<_> = keys.iter().map(shown).collect();
                card.rows.push(("Keys".into(), keys.join(", ")));
            }
        }
        "write" => {
            if let Some(path) = rest.remove("path") {
                card.rows.push(("File".into(), shown(&path)));
            }
            if let Some(text) = rest.remove("text") {
                let before = proposal["diff"]["before"].as_str().unwrap_or("");
                card.diff = diff(before, text.as_str().unwrap_or(""));
            }
        }
        "http" => {
            if let (Some(method), Some(url)) = (arguments.get("method"), arguments.get("url")) {
                card.rows.push((
                    "Request".into(),
                    format!("{} {}", shown(method), shown(url)),
                ));
                rest.remove("method");
                rest.remove("url");
            }
            if rest.get("body").is_some_and(Value::is_null) {
                rest.remove("body");
            }
        }
        _ => {}
    }
    for (key, value) in rest {
        card.rows.push((label(&key), shown(&value)));
    }
    for (key, value) in proposal.as_object().into_iter().flatten() {
        if !matches!(
            key.as_str(),
            "tool" | "arguments" | "diff" | "keys" | "before" | "_forecast_model"
        ) {
            card.rows.push((label(key), shown(value)));
        }
    }
    card
}

/// `desktop_id` as "Desktop id".
fn label(key: &str) -> String {
    let mut words = visible(&key.replace('_', " "));
    if let Some(first) = words.get(..1) {
        words = first.to_uppercase() + &words[1..];
    }
    words
}

/// The open or finished task a reply reports; `None` when the reply registered no task.
#[derive(Debug, PartialEq)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    /// `None` while the task is open.
    pub outcome: Option<String>,
    pub warning: Option<String>,
    /// (criterion, verdict, description)
    pub criteria: Vec<(String, String, String)>,
}

pub fn task(status: &Value) -> Option<TaskView> {
    if status.is_null() || status["registered"] == false {
        return None;
    }
    if status["integrity"] != "checked" {
        return Some(TaskView {
            id: String::new(),
            title: "Task evidence".into(),
            outcome: None,
            warning: Some(visible(&format!(
                "Task evidence is unavailable, so the result is unverified. {}",
                status["error"].as_str().unwrap_or("")
            ))),
            criteria: Vec::new(),
        });
    }
    let task = status["tasks"].as_array()?.last()?;
    let checks = task["checks"].as_array().cloned().unwrap_or_default();
    let criteria = task["task"]["definition"]["criteria"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|criterion| {
            let check = checks.iter().find(|c| c["criterion"] == criterion["id"]);
            (
                visible(criterion["id"].as_str().unwrap_or("?")),
                visible(
                    check
                        .and_then(|c| c["verdict"].as_str())
                        .unwrap_or("unchecked"),
                ),
                visible(criterion["description"].as_str().unwrap_or("")),
            )
        })
        .collect();
    Some(TaskView {
        id: visible(task["task"]["id"].as_str().unwrap_or("?")),
        title: visible(
            task["task"]["definition"]["title"]
                .as_str()
                .unwrap_or("Task"),
        ),
        outcome: task["outcome"]["verdict"].as_str().map(visible),
        warning: None,
        criteria,
    })
}

/// Model IDs from a provider catalog: Ollama's `models[].name` or `data[].id`.
pub fn catalog(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str())
        .chain(
            value["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str()),
        )
        .map(visible)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// A reply's retrieved memories: (source · short ID, excerpt).
pub fn memories(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let id = m["id"].as_str().unwrap_or("");
            let short: String = id.chars().take(12).collect();
            let more = if id.chars().count() > 12 { "…" } else { "" };
            (
                visible(&format!(
                    "{} · {short}{more}",
                    m["source"].as_str().unwrap_or("memory")
                )),
                visible(m["excerpt"].as_str().unwrap_or("")),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLES: &[&str] = &[
        "Hi **there** and `x<y>`",
        "<span foreground='red'>x</span> & ** \"quoted\"",
        "**** and ``` inline and `unclosed",
        "# Title\n\n- one\n  - two **b**\n3. three\n\n```\n<b>raw</b> & code\n```\ntail",
        "```\nnever closed <i>",
        "\u{202e}reversed \u{200b}hidden",
    ];

    #[test]
    fn markup_escapes_everything_and_renders_a_small_subset() {
        assert_eq!(
            markup(SAMPLES[0]),
            [Block::Text(
                "Hi <b>there</b> and <tt>x&lt;y&gt;</tt>".into()
            )]
        );
        assert_eq!(
            markup(SAMPLES[1]),
            [Block::Text(
                "&lt;span foreground=&#39;red&#39;&gt;x&lt;/span&gt; &amp; ** &quot;quoted&quot;"
                    .into()
            )]
        );
        assert_eq!(
            markup(SAMPLES[3]),
            [
                Block::Heading("Title".into()),
                Block::Item {
                    marker: "•".into(),
                    depth: 0,
                    markup: "one".into()
                },
                Block::Item {
                    marker: "•".into(),
                    depth: 1,
                    markup: "two <b>b</b>".into()
                },
                Block::Item {
                    marker: "3.".into(),
                    depth: 0,
                    markup: "three".into()
                },
                Block::Code("<b>raw</b> & code".into()),
                Block::Text("tail".into()),
            ]
        );
        assert_eq!(markup(SAMPLES[4]), [Block::Code("never closed <i>".into())]);
        // Invisible and reordering characters are shown as escapes, never applied.
        assert_eq!(
            markup(SAMPLES[5]),
            [Block::Text("\\u{202e}reversed \\u{200b}hidden".into())]
        );
    }

    #[cfg(all(feature = "desktop-ui", target_os = "linux"))]
    #[test]
    fn every_markup_block_parses_as_pango_markup() {
        for sample in SAMPLES {
            for block in markup(sample) {
                let text = match block {
                    Block::Text(m) | Block::Heading(m) | Block::Item { markup: m, .. } => m,
                    Block::Code(_) => continue,
                };
                gtk4::pango::parse_markup(&text, '\0').unwrap_or_else(|e| panic!("{text}: {e}"));
            }
        }
    }

    fn strings(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) => out.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
            Value::Object(map) => map.values().for_each(|v| strings(v, out)),
            other => out.push(other.to_string()),
        }
    }

    #[test]
    fn approval_cards_show_every_argument_verbatim() {
        let fixtures = [
            json!({"tool":"command","arguments":{"argv":["sh","-c","echo 'hi there' > out.txt",""]}}),
            json!({"tool":"write","arguments":{"path":"notes/a.md","text":"line one\nline <two>"},
                "diff":{"path":"notes/a.md","before":"","after":"line one\nline <two>"}}),
            json!({"tool":"http","arguments":{"method":"POST","url":"https://example.invalid/x?y=1",
                "body":{"name":"Lucy","list":[1,2]}}}),
            json!({"tool":"service","arguments":{"name":"ollama.service","operation":"restart","system":true}}),
            json!({"tool":"launch","arguments":{"desktop_id":"org.kde.dolphin"}}),
            json!({"tool":"window","arguments":{"action":"close","address":"0x55d1"}}),
            json!({"tool":"gui","arguments":{"argv":["key","30:1","30:0"]},"keys":["A down","A up"]}),
            json!({"tool":"privileged","arguments":{"operation":"hardware-info"}}),
            json!({"tool":"future","arguments":{"surprise":"value"},"note":"extra field"}),
        ];
        for proposal in fixtures {
            let card = super::proposal(&proposal);
            assert!(card.title.starts_with("Lucy asks to"), "{}", card.title);
            let shown: String = card
                .rows
                .iter()
                .map(|(k, v)| format!("{k}\n{v}\n"))
                .chain(card.diff.iter().map(|line| format!("{line:?}\n")))
                .collect();
            let mut expected = Vec::new();
            strings(&proposal["arguments"], &mut expected);
            strings(&proposal["keys"], &mut expected);
            if let Some(note) = proposal.get("note") {
                strings(note, &mut expected);
            }
            for value in expected.iter().filter(|v| v.as_str() != "null") {
                let quoted = Value::String(value.clone()).to_string();
                let lines_shown = value
                    .lines()
                    .all(|line| shown.contains(&format!("{line:?}")));
                assert!(
                    shown.contains(value.as_str()) || shown.contains(&quoted) || lines_shown,
                    "{value:?} missing from {shown}"
                );
            }
            assert!(card.raw.contains("\"tool\""));
        }
        assert_eq!(
            super::proposal(&json!({"tool":"command","arguments":{"argv":["ls","-la","a b"]}}))
                .rows,
            [("Command".to_owned(), "ls -la \"a b\"".to_owned())]
        );
    }

    #[test]
    fn diffs_frame_every_changed_line() {
        let before = "a\nb\nc\nd\ne\nf\ng";
        assert_eq!(
            diff(before, "a\nb\nc\nX\nY\ne\nf\ng"),
            [
                Line::Skipped(1),
                Line::Same("b".into()),
                Line::Same("c".into()),
                Line::Removed("d".into()),
                Line::Added("X".into()),
                Line::Added("Y".into()),
                Line::Same("e".into()),
                Line::Same("f".into()),
                Line::Skipped(1),
            ]
        );
        assert_eq!(diff("", "new"), [Line::Added("new".into())]);
        assert_eq!(
            diff("same", "same\n"),
            [
                Line::Same("same".into()),
                Line::Note("Only line endings or the final newline change.")
            ]
        );
    }

    #[test]
    fn task_views_follow_the_status_and_skip_ordinary_chat() {
        assert_eq!(task(&json!({"registered":false})), None);
        assert_eq!(task(&Value::Null), None);
        assert!(
            task(&json!({"integrity":"unavailable","error":"store busy"}))
                .unwrap()
                .warning
                .unwrap()
                .contains("store busy")
        );
        let status = json!({"integrity":"checked","tasks":[{
            "task":{"id":"task-1","definition":{"title":"Write it","criteria":[
                {"id":"content","description":"The file holds the answer"},
                {"id":"tests","description":"Tests pass"}]}},
            "checks":[{"criterion":"content","verdict":"success"}]}]});
        let view = task(&status).unwrap();
        assert_eq!(view.outcome, None);
        assert_eq!(view.title, "Write it");
        assert_eq!(
            view.criteria,
            [
                (
                    "content".into(),
                    "success".into(),
                    "The file holds the answer".into()
                ),
                ("tests".into(), "unchecked".into(), "Tests pass".into())
            ]
        );
        let mut done = status.clone();
        done["tasks"][0]["outcome"] = json!({"verdict":"inconclusive"});
        assert_eq!(
            task(&done).unwrap().outcome.as_deref(),
            Some("inconclusive")
        );
    }

    #[test]
    fn catalogs_and_memories_are_listed_plainly() {
        assert_eq!(
            catalog(&json!({"models":[{"name":"qwen3.6:35b"},{"name":"embeddinggemma"}]})),
            ["embeddinggemma", "qwen3.6:35b"]
        );
        assert_eq!(
            catalog(&json!({"data":[{"id":"openrouter/free"}]})),
            ["openrouter/free"]
        );
        assert!(catalog(&json!({"unexpected":true})).is_empty());
        assert_eq!(
            memories(
                &json!([{"source":"claude","id":"0123456789abcdef","excerpt":"We chose\u{202e}"}])
            ),
            [(
                "claude · 0123456789ab…".to_owned(),
                "We chose\\u{202e}".to_owned()
            )]
        );
    }
}
