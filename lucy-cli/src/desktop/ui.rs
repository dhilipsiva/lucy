//! The Ask Lucy window. GTK objects stay on the main thread; service requests run on worker
//! threads and come back through a channel. Model, memory and proposal text is shaped by
//! `view` and shown only as escaped markup or plain text.
use super::view::{self, Block, Line};
use super::{Result, call, call_within, err, token};
use gtk4::{self as gtk, gdk, gio, glib, pango, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

const PROVIDERS: [(&str, &str); 5] = [
    ("ollama", "Local (Ollama)"),
    ("nvidia", "NVIDIA"),
    ("groq", "Groq"),
    ("gemini", "Gemini"),
    ("openrouter", "OpenRouter"),
];
/// The model list's first entry for a hosted provider, until a model is chosen.
const CHOOSE: &str = "Choose a model…";
const NOTICE: &str = "Local Ollama is the default. Choosing a hosted provider sends your prompts, retrieved memories and tool results to it. Free access depends on the model and account; Lucy does not switch providers automatically.";
/// QUINE's single accent, for the few marks set in markup rather than the stylesheet.
const EMBER: &str = "#F2542D";
const SUGGESTIONS: [&str; 3] = [
    "What were we working on last?",
    "What do you remember about this project?",
    "Look around this folder and tell me what it is.",
];

struct Reply {
    generation: u64,
    kind: &'static str,
    result: Result<Value>,
}

#[derive(Clone, Copy)]
enum Status {
    Asleep,
    Ready,
    Thinking,
    Approval,
    Stopped,
    Problem,
}

struct Window {
    client: Value,
    workspace: PathBuf,
    local_model: String,
    /// Conversation requests only: a reply from before Stop or Sleep is dropped.
    generation: Cell<u64>,
    sender: mpsc::Sender<Reply>,
    busy: Cell<Option<(Instant, &'static str)>>,
    shown_seconds: Cell<u64>,
    /// The conversation keeps to its end until then, while new content is laid out.
    pinned: Rc<Cell<Option<Instant>>>,
    pill: gtk::Box,
    pill_dot: gtk::Box,
    pill_label: gtk::Label,
    pill_spinner: gtk::Spinner,
    model_button: gtk::MenuButton,
    banner: gtk::Revealer,
    banner_label: gtk::Label,
    messages: gtk::Box,
    empty: RefCell<Option<gtk::Box>>,
    thinking: gtk::Box,
    thinking_label: gtk::Label,
    /// Approval cards still waiting: (action ID, their button row).
    cards: RefCell<Vec<(String, gtk::Box)>>,
    taskbar: gtk::Revealer,
    task_rows: gtk::Box,
    finished: RefCell<Vec<String>>,
    entry: gtk::TextView,
    send: gtk::Button,
    settings: gtk::Popover,
    provider: gtk::DropDown,
    /// Offers `models`: for a hosted provider, `CHOOSE` and then its catalog.
    model: gtk::DropDown,
    models: gtk::StringList,
    model_status: gtk::Label,
    reload: gtk::Button,
    key_row: gtk::Box,
    key_status: gtk::Label,
    key: gtk::PasswordEntry,
    choose: gtk::Button,
    known: RefCell<Value>,
    /// Catalogs fetched while the window is open, by provider.
    catalogs: RefCell<BTreeMap<String, Vec<String>>>,
}

/// A provider's name as the settings list shows it.
fn title(id: &str) -> &str {
    PROVIDERS
        .iter()
        .find(|(known, _)| *known == id)
        .map_or(id, |(_, name)| name)
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .build();
    if !class.is_empty() {
        label.add_css_class(class);
    }
    label
}

/// Selectable text, set as plain text: nothing in it is interpreted.
fn plain(text: &str, class: &str) -> gtk::Label {
    let label = label("", class);
    label.set_text(text);
    label.set_selectable(true);
    label
}

/// Markup built by `view` from escaped text.
fn rich(markup: &str, class: &str) -> gtk::Label {
    let label = label("", class);
    label.set_markup(markup);
    label.set_selectable(true);
    label.set_halign(gtk::Align::Start);
    label.set_max_width_chars(92);
    label
}

/// A QUINE eyebrow: an ember `//`, then the words, escaped.
fn eyebrow(words: &str) -> gtk::Label {
    let eyebrow = label("", "eyebrow");
    eyebrow.set_wrap(false);
    eyebrow.set_markup(&format!(
        "<span foreground=\"{EMBER}\">//</span> {}",
        view::escape(words)
    ));
    eyebrow
}

fn avatar(size: &str) -> gtk::Label {
    let avatar = gtk::Label::new(Some("L"));
    avatar.add_css_class("avatar");
    if !size.is_empty() {
        avatar.add_css_class(size);
    }
    avatar.set_valign(gtk::Align::Start);
    avatar.set_halign(gtk::Align::Center);
    avatar
}

fn chip(text: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    button.add_css_class("chip");
    button
}

fn copy(text: String) -> gtk::Button {
    let button = chip("Copy");
    button.set_tooltip_text(Some("Copy to the clipboard"));
    button.connect_clicked(move |button| {
        button.clipboard().set_text(&text);
        button.set_label("Copied");
    });
    button
}

fn scroll(child: &impl IsA<gtk::Widget>, height: i32) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(child)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(height)
        .build()
}

/// A text view's natural height is not known before it has a width, so a scrolled one gets
/// room for its lines up front, up to `most` pixels.
fn sized(view: &gtk::TextView, lines: usize, most: i32) -> gtk::ScrolledWindow {
    let scrolled = scroll(view, most);
    scrolled.set_min_content_height((lines as i32 * 19 + 18).min(most));
    scrolled
}

fn text_view(class: &str) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::Char)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(10)
        .right_margin(10)
        .build();
    view.add_css_class(class);
    view
}

fn block(block: Block) -> gtk::Widget {
    match block {
        Block::Text(markup) => rich(&markup, "body").upcast(),
        Block::Heading(markup) => rich(&markup, "heading").upcast(),
        Block::Item {
            marker,
            depth,
            markup,
        } => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_margin_start(depth as i32 * 18);
            let marker = label(&marker, "marker");
            marker.set_wrap(false);
            marker.set_valign(gtk::Align::Start);
            row.append(&marker);
            row.append(&rich(&markup, "body"));
            row.upcast()
        }
        Block::Code(text) => {
            let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
            frame.add_css_class("code");
            let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            bar.add_css_class("codebar");
            let kind = label("CODE", "");
            kind.set_hexpand(true);
            bar.append(&kind);
            bar.append(&copy(text.clone()));
            frame.append(&bar);
            let body = plain(&text, "mono");
            body.set_wrap_mode(pango::WrapMode::Char);
            frame.append(&body);
            frame.upcast()
        }
    }
}

/// A proposed file change, every changed line shown, in one scrolling text view.
fn diff(lines: &[Line]) -> gtk::Widget {
    let view = text_view("diff");
    let buffer = view.buffer();
    for (name, color) in [
        ("added", gdk::RGBA::new(0.13, 0.66, 0.45, 0.18)),
        ("removed", gdk::RGBA::new(0.86, 0.33, 0.24, 0.18)),
    ] {
        let tag = gtk::TextTag::builder()
            .name(name)
            .paragraph_background_rgba(&color)
            .build();
        buffer.tag_table().add(&tag);
    }
    let quiet = gtk::TextTag::builder()
        .name("quiet")
        .foreground_rgba(&gdk::RGBA::new(0.55, 0.59, 0.64, 1.0))
        .build();
    buffer.tag_table().add(&quiet);
    for (n, line) in lines.iter().enumerate() {
        let (text, tag) = match line {
            Line::Same(text) => (format!("  {text}"), None),
            Line::Removed(text) => (format!("− {text}"), Some("removed")),
            Line::Added(text) => (format!("+ {text}"), Some("added")),
            Line::Skipped(1) => ("… 1 unchanged line".to_owned(), Some("quiet")),
            Line::Skipped(count) => (format!("… {count} unchanged lines"), Some("quiet")),
            Line::Note(note) => ((*note).to_owned(), Some("quiet")),
        };
        let text = if n + 1 < lines.len() {
            text + "\n"
        } else {
            text
        };
        buffer.insert_with_tags_by_name(&mut buffer.end_iter(), &text, tag.as_slice());
    }
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("diffbox");
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.append(&sized(&view, lines.len(), 300));
    frame.upcast()
}

/// Replaces a card's buttons once it is decided or withdrawn.
fn settle(actions: &gtk::Box, text: &str) {
    while let Some(child) = actions.first_child() {
        actions.remove(&child);
    }
    actions.append(&label(text, "decision"));
}

fn criteria(rows: &gtk::Box, task: &view::TaskView) {
    for (_, verdict, description) in &task.criteria {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let mark = match verdict.as_str() {
            "success" => "✓",
            "failure" => "✗",
            "inconclusive" => "?",
            _ => "○",
        };
        let mark = label(mark, "verdict");
        mark.add_css_class(match verdict.as_str() {
            known @ ("success" | "failure" | "inconclusive") => known,
            _ => "unchecked",
        });
        mark.set_valign(gtk::Align::Start);
        mark.set_wrap(false);
        row.append(&mark);
        let text = label(description, "criterion");
        text.set_hexpand(true);
        text.set_tooltip_text(Some(&format!("{verdict}: {description}")));
        row.append(&text);
        rows.append(&row);
    }
}

/// `/home/me/LucyTasks` as `~/LucyTasks`.
fn home(path: &Path) -> String {
    let inside = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf));
    super::visible(&match inside {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    })
}

impl Window {
    fn status(&self, status: Status) {
        for class in ["ready", "thinking", "approval", "problem"] {
            self.pill.remove_css_class(class);
        }
        let (class, text) = match status {
            Status::Asleep => ("", "ASLEEP"),
            Status::Ready => ("ready", "READY"),
            Status::Thinking => ("thinking", "THINKING"),
            Status::Approval => ("approval", "NEEDS APPROVAL"),
            Status::Stopped => ("", "STOPPED"),
            Status::Problem => ("problem", "PROBLEM"),
        };
        if !class.is_empty() {
            self.pill.add_css_class(class);
        }
        self.pill_label.set_text(text);
        let thinking = matches!(status, Status::Thinking);
        self.pill_dot.set_visible(!thinking);
        self.pill_spinner.set_visible(thinking);
        self.pill_spinner.set_spinning(thinking);
    }

    fn problem(&self, error: &str) {
        self.banner_label.set_text(&super::visible(error.trim()));
        self.banner.set_reveal_child(true);
        self.pin();
        self.status(Status::Problem);
    }

    fn pin(&self) {
        self.pinned
            .set(Some(Instant::now() + Duration::from_millis(1500)));
    }

    /// Appends to the conversation, keeping the thinking row last while Lucy works.
    fn add(&self, widget: &impl IsA<gtk::Widget>) {
        self.pin();
        if let Some(empty) = self.empty.take() {
            self.messages.remove(&empty);
        }
        if self.thinking.parent().is_some() {
            self.messages.remove(&self.thinking);
        }
        self.messages.append(widget);
        if self.busy.get().is_some() {
            self.messages.append(&self.thinking);
        }
    }

    fn working(&self, what: Option<&'static str>) {
        self.busy.set(what.map(|what| (Instant::now(), what)));
        self.shown_seconds.set(0);
        match what {
            Some(what) => {
                self.thinking_label.set_text(what);
                if let Some(empty) = self.empty.take() {
                    self.messages.remove(&empty);
                }
                if self.thinking.parent().is_none() {
                    self.messages.append(&self.thinking);
                }
                self.pin();
                self.status(Status::Thinking);
                self.send.set_label("Stop");
                self.send.add_css_class("stop");
                self.send
                    .set_tooltip_text(Some("Stop what Lucy is doing now"));
            }
            None => {
                if self.thinking.parent().is_some() {
                    self.messages.remove(&self.thinking);
                }
                self.send.set_label("Send →");
                self.send.remove_css_class("stop");
                self.send.set_tooltip_text(Some("Send (Enter)"));
            }
        }
    }

    fn tick(&self) {
        if let Some((since, what)) = self.busy.get() {
            let seconds = since.elapsed().as_secs();
            if seconds != self.shown_seconds.get() {
                self.shown_seconds.set(seconds);
                self.thinking_label.set_text(&format!("{what} {seconds} s"));
            }
        }
    }

    fn note(&self, text: &str) {
        let note = label(text, "note");
        note.set_halign(gtk::Align::Center);
        note.set_xalign(0.5);
        note.set_justify(gtk::Justification::Center);
        self.add(&note);
    }

    fn mine(&self, text: &str) {
        let bubble = plain(&super::visible(text), "bubble");
        bubble.set_halign(gtk::Align::End);
        bubble.set_max_width_chars(64);
        bubble.set_margin_start(72);
        self.add(&bubble);
    }

    fn reply(&self, text: &str, memories: &Value) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("reply");
        row.append(&avatar("small"));
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.set_hexpand(true);
        let said = gtk::Box::new(gtk::Orientation::Vertical, 8);
        said.add_css_class("said");
        said.set_halign(gtk::Align::Start);
        for part in view::markup(text) {
            said.append(&block(part));
        }
        column.append(&said);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        // As in Lucy's room: what a model says is labelled words, never a verdict.
        let words = label("IN WORDS", "badge");
        words.add_css_class("words");
        words.set_wrap(false);
        words.set_valign(gtk::Align::Center);
        words.set_tooltip_text(Some(
            "Written by the model: words, not a verdict from the engine",
        ));
        footer.append(&words);
        footer.append(&copy(super::visible(text)));
        column.append(&footer);
        let found = view::memories(memories);
        if !found.is_empty() {
            let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
            list.set_margin_top(4);
            for (source, excerpt) in &found {
                let memory = gtk::Box::new(gtk::Orientation::Vertical, 3);
                memory.add_css_class("memory");
                let source = plain(source, "source");
                source.add_css_class("mono");
                memory.append(&source);
                memory.append(&plain(excerpt, "excerpt"));
                list.append(&memory);
            }
            let shown = gtk::Revealer::builder()
                .child(&list)
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .build();
            let count = match found.len() {
                1 => "1 memory used".to_owned(),
                n => format!("{n} memories used"),
            };
            let toggle = gtk::ToggleButton::with_label(&format!("{count} ▸"));
            toggle.add_css_class("chip");
            toggle.set_tooltip_text(Some("What Lucy recalled for this answer"));
            let revealer = shown.clone();
            toggle.connect_toggled(move |toggle| {
                let open = toggle.is_active();
                revealer.set_reveal_child(open);
                toggle.set_label(&format!("{count} {}", if open { "▾" } else { "▸" }));
            });
            footer.append(&toggle);
            column.append(&shown);
        }
        row.append(&column);
        self.add(&row);
    }

    fn approval(self: &Rc<Self>, id: &str, proposal: &Value) {
        let card = view::proposal(proposal);
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 10);
        frame.add_css_class("card");
        frame.add_css_class("approval");
        frame.append(&eyebrow("needs your approval"));
        frame.append(&label(&card.title, "card-title"));
        frame.append(&label("Nothing runs until you approve it.", "note"));
        if !card.rows.is_empty() {
            let grid = gtk::Grid::builder()
                .column_spacing(14)
                .row_spacing(6)
                .build();
            for (row, (key, value)) in card.rows.iter().enumerate() {
                let key = label(&key.to_uppercase(), "key");
                key.set_wrap(false);
                key.set_valign(gtk::Align::Start);
                let value = plain(value, "value");
                value.add_css_class("mono");
                value.set_wrap_mode(pango::WrapMode::Char);
                value.set_hexpand(true);
                grid.attach(&key, 0, row as i32, 1, 1);
                grid.attach(&value, 1, row as i32, 1, 1);
            }
            frame.append(&grid);
        }
        if !card.diff.is_empty() {
            frame.append(&diff(&card.diff));
        }
        let raw = text_view("raw");
        raw.buffer().set_text(&card.raw);
        frame.append(
            &gtk::Expander::builder()
                .label("Raw request")
                .child(&sized(&raw, card.raw.lines().count(), 240))
                .build(),
        );
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let deny = gtk::Button::with_label("Deny");
        deny.add_css_class("deny");
        let approve = gtk::Button::with_label("Approve");
        approve.add_css_class("approve");
        // Enter belongs to the composer: it never approves.
        approve.set_receives_default(false);
        for (button, allow) in [(&deny, false), (&approve, true)] {
            let state = Rc::downgrade(self);
            let (id, row) = (id.to_owned(), actions.clone());
            button.connect_clicked(move |_| {
                if let Some(state) = state.upgrade() {
                    state.cards.borrow_mut().retain(|(open, _)| *open != id);
                    settle(&row, if allow { "Approved." } else { "Denied." });
                    state.entry.grab_focus();
                    state.work(json!({"op":"approve","action":id,"allow":allow}), "chat");
                }
            });
            actions.append(button);
        }
        frame.append(&actions);
        self.cards.borrow_mut().push((id.to_owned(), actions));
        self.add(&frame);
    }

    /// Cards whose proposal the service no longer holds can't be approved any more.
    fn withdraw(&self, keep: &[String]) {
        self.cards.borrow_mut().retain(|(id, actions)| {
            let kept = keep.contains(id);
            if !kept {
                settle(actions, "Withdrawn: no longer waiting for you.");
            }
            kept
        });
    }

    fn task(&self, status: &Value) {
        let Some(task) = view::task(status) else {
            return;
        };
        if let Some(warning) = &task.warning {
            self.note(warning);
            return;
        }
        match &task.outcome {
            None => {
                while let Some(child) = self.task_rows.first_child() {
                    self.task_rows.remove(&child);
                }
                let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                heading.append(&eyebrow("task"));
                let title = label(&task.title, "muted");
                title.set_wrap(false);
                title.set_ellipsize(pango::EllipsizeMode::End);
                title.set_hexpand(true);
                heading.append(&title);
                self.task_rows.append(&heading);
                criteria(&self.task_rows, &task);
                self.taskbar.set_reveal_child(true);
            }
            Some(outcome) => {
                self.taskbar.set_reveal_child(false);
                if self.finished.borrow().contains(&task.id) {
                    return;
                }
                self.finished.borrow_mut().push(task.id.clone());
                let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
                card.add_css_class("card");
                card.add_css_class("outcome");
                card.add_css_class(match outcome.as_str() {
                    "success" => "success",
                    "failure" => "failure",
                    _ => "inconclusive",
                });
                card.append(&eyebrow("task finished"));
                card.append(&label(
                    match outcome.as_str() {
                        "success" => "Task done, and every check passed",
                        "failure" => "Task finished, but a check failed",
                        _ => "Task ended without full evidence",
                    },
                    "card-title",
                ));
                card.append(&plain(&task.title, "muted"));
                criteria(&card, &task);
                self.add(&card);
            }
        }
    }

    fn work(&self, mut request: Value, kind: &'static str) {
        let conversation = matches!(kind, "chat" | "control");
        if request["op"] == "chat" {
            request["announce_task"] = json!(true);
        }
        let generation = if conversation {
            self.generation.set(self.generation.get() + 1);
            self.generation.get()
        } else {
            0
        };
        let request = view::addressed(request, &self.client);
        if kind == "chat" {
            self.banner.set_reveal_child(false);
            self.working(Some(match request["op"].as_str() {
                Some("approve") if request["allow"] == true => "Running what you approved…",
                Some("continue-task") => "Lucy is working on the task…",
                _ => "Lucy is thinking…",
            }));
        }
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = if kind == "select" {
                (|| {
                    if request["name"] != "ollama" {
                        let mut configure = request.clone();
                        configure["op"] = json!("provider-config");
                        call(configure)?;
                    }
                    call(request)
                })()
            } else if kind == "catalog" {
                // Tagged with its provider, so a late catalog cannot fill another's list.
                let name = request["name"].clone();
                Ok(match call(request) {
                    Ok(catalog) => json!({"name":name,"models":view::catalog(&catalog)}),
                    Err(error) => json!({"name":name,"error":error}),
                })
            } else {
                call(request)
            };
            let _ = sender.send(Reply {
                generation,
                kind,
                result,
            });
        });
    }

    fn send_text(&self) {
        if self.busy.get().is_some() {
            return;
        }
        let buffer = self.entry.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        if text.trim().is_empty() {
            return;
        }
        match token().and_then(|id| {
            std::fs::create_dir_all(&self.workspace).map_err(err)?;
            Ok(id)
        }) {
            Ok(event) => {
                self.mine(&text);
                buffer.set_text("");
                self.work(
                    json!({"op":"chat","text":text,"event":event,"workspace":self.workspace}),
                    "chat",
                );
            }
            Err(error) => self.problem(&error),
        }
    }

    fn providers(&self, result: &Value, chosen: bool) {
        let selected = result["selected"].as_str().unwrap_or("ollama");
        // Known first: changing the provider shown reads it.
        *self.known.borrow_mut() = result.clone();
        let index = PROVIDERS
            .iter()
            .position(|(id, _)| *id == selected)
            .unwrap_or(0) as u32;
        if self.provider.selected() == index {
            self.show_provider();
        } else {
            self.provider.set_selected(index);
        }
        let model = self.configured(selected);
        let name = if selected == "ollama" {
            "Local".to_owned()
        } else {
            title(selected).to_owned()
        };
        let full = super::visible(&format!("{name} · {model}"));
        let short: String = full.chars().take(30).collect();
        self.model_button.set_label(&if short.len() < full.len() {
            format!("{short}…")
        } else {
            full.clone()
        });
        self.model_button
            .set_tooltip_text(Some(&format!("{full}\nModel and provider settings")));
        if chosen {
            self.note(&format!("Now using {full}."));
        }
    }

    /// The model the service has configured for a provider.
    fn configured(&self, name: &str) -> String {
        let known = self.known.borrow();
        known["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(known["presets"].as_array().into_iter().flatten())
            .filter(|p| p["name"] == name)
            .find_map(|p| p["model"].as_str())
            .map(super::visible)
            .unwrap_or_else(|| {
                if name == "ollama" {
                    self.local_model.clone()
                } else {
                    String::new()
                }
            })
    }

    /// The provider in the settings list, which need not be the one in use.
    fn current(&self) -> &'static str {
        PROVIDERS[self.provider.selected() as usize % PROVIDERS.len()].0
    }

    /// A hosted preset's field from the service's last provider list.
    fn preset(&self, name: &str, field: &str) -> Option<String> {
        self.known.borrow()["presets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name)
            .and_then(|p| p[field].as_str())
            .map(String::from)
    }

    /// The model chosen in settings. A hosted provider's first entry is `CHOOSE`.
    fn chosen_model(&self) -> Option<String> {
        let index = self.model.selected();
        if index == gtk::INVALID_LIST_POSITION || (self.current() != "ollama" && index == 0) {
            return None;
        }
        self.models.string(index).map(String::from)
    }

    fn offer(&self, items: &[&str], selected: u32) {
        self.models.splice(0, self.models.n_items(), items);
        self.model.set_selected(selected);
        self.choose.set_sensitive(self.chosen_model().is_some());
    }

    /// Settings for the provider in the list: its key, and its models once it has one.
    /// Catalogs are fetched only while the settings are open.
    fn show_provider(&self) {
        let name = self.current();
        self.key_row.set_visible(name != "ollama");
        if name == "ollama" {
            // The host's configuration chooses the local model.
            self.offer(&[self.local_model.as_str()], 0);
            self.model.set_sensitive(false);
            self.reload.set_visible(false);
            self.model_status
                .set_text("The local model is set by this computer's configuration.");
            return;
        }
        let source = self.preset(name, "key");
        let key_env = self.preset(name, "key_env").unwrap_or_default();
        self.key_status
            .set_text(&view::key_note(source.as_deref(), &key_env));
        self.key.set_placeholder_text(Some(if source.is_some() {
            "Paste a new key to replace it"
        } else {
            "API key (saved locally)"
        }));
        self.model.set_sensitive(true);
        let cached = self.catalogs.borrow().get(name).cloned();
        if let Some(ids) = cached {
            self.show_catalog(name, &ids);
            return;
        }
        self.offer(&[CHOOSE], 0);
        self.reload.set_visible(false);
        if source.is_none() {
            self.model_status
                .set_text("Save an API key to list this provider's models.");
        } else if self.settings.is_visible() {
            self.model_status
                .set_text(&format!("Loading {}'s models…", title(name)));
            self.work(json!({"op":"provider-models","name":name}), "catalog");
        } else {
            self.model_status.set_text("");
        }
    }

    fn show_catalog(&self, name: &str, ids: &[String]) {
        let saved = self.configured(name);
        let mut items = vec![CHOOSE];
        items.extend(ids.iter().map(String::as_str));
        let position = ids.iter().position(|id| *id == saved);
        self.offer(&items, position.map_or(0, |p| p as u32 + 1));
        self.reload.set_visible(true);
        self.model_status.set_text(&if ids.is_empty() {
            "The provider sent no model list.".to_owned()
        } else if position.is_none() && !saved.is_empty() {
            format!(
                "The saved model {saved} is not in {}'s catalog. Choose one.",
                title(name)
            )
        } else {
            format!("{} models from {}.", ids.len(), title(name))
        });
    }

    fn fill_catalog(&self, result: &Value) {
        let name = result["name"].as_str().unwrap_or_default();
        if let Some(error) = result["error"].as_str() {
            if name == self.current() {
                self.offer(&[CHOOSE], 0);
                self.model_status.set_text(&super::visible(error));
                self.reload.set_visible(true);
            }
            return;
        }
        let listed = result["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(String::from))
            .collect();
        let ids = view::model_choices(listed, self.preset(name, "model").as_deref());
        self.catalogs.borrow_mut().insert(name.into(), ids);
        if name == self.current() {
            self.show_provider();
        }
    }

    fn save_key(&self) {
        let key = self.key.text();
        if key.trim().is_empty() {
            return;
        }
        let request = json!({"op":"provider-key","name":self.current(),"key":key.as_str()});
        self.key.set_text("");
        self.key_status.set_text("Saving the key…");
        self.work(request, "key");
    }

    fn finish(self: &Rc<Self>, reply: Reply) {
        let conversation = matches!(reply.kind, "chat" | "control");
        if conversation && reply.generation != self.generation.get() {
            return;
        }
        let result = match reply.result {
            Ok(result) => result,
            Err(error) if reply.kind == "key" => {
                self.key_status.set_text(&super::visible(&error));
                return;
            }
            Err(error) => {
                if conversation {
                    self.working(None);
                    self.withdraw(&[]);
                }
                // A failed turn reports its task evidence after the error.
                let (message, evidence) = error
                    .split_once("\nTask evidence (data): ")
                    .unwrap_or((&error, ""));
                self.problem(message);
                if let Ok(status) = serde_json::from_str::<Value>(evidence) {
                    self.task(&status);
                }
                return;
            }
        };
        match reply.kind {
            "providers" | "select" => {
                self.providers(&result, reply.kind == "select");
                return;
            }
            "catalog" => {
                self.fill_catalog(&result);
                return;
            }
            "key" => {
                // A new key may list other models; the provider in use stays as it is.
                *self.known.borrow_mut() = result;
                self.catalogs.borrow_mut().clear();
                self.show_provider();
                return;
            }
            "control" => {
                self.working(None);
                self.withdraw(&[]);
                self.taskbar.set_reveal_child(false);
                if result["active"] == true {
                    self.note("Stopped. Send a message to carry on.");
                    self.status(Status::Stopped);
                } else {
                    self.note("Lucy is asleep. Send a message to wake her.");
                    self.status(Status::Asleep);
                }
                return;
            }
            _ => {}
        }
        self.working(None);
        if let Some(text) = result["text"].as_str().filter(|s| !s.trim().is_empty()) {
            self.reply(text, &result["memories"]);
        }
        if let Some(status) = result.get("task_status") {
            self.task(status);
        }
        if let Some(continuation) = result["continue_task"].as_str() {
            self.work(
                json!({"op":"continue-task","continuation":continuation}),
                "chat",
            );
            return;
        }
        let pending: Vec<String> = result["pending"]
            .as_object()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default();
        self.withdraw(&pending);
        for (id, proposal) in result["pending"].as_object().into_iter().flatten() {
            if !self.cards.borrow().iter().any(|(open, _)| open == id) {
                self.approval(id, proposal);
            }
        }
        self.status(if !pending.is_empty() {
            Status::Approval
        } else if result["active"] == true {
            Status::Ready
        } else {
            Status::Asleep
        });
    }
}

/// GTK 4.20 answers a stylesheet's `prefers-color-scheme` from that stylesheet's own setting,
/// so it follows the desktop here. QUINE is dark-first: the window is dark unless the desktop
/// asks for light, and older GTK keeps it dark.
fn follow_scheme(settings: &gtk::Settings, provider: &gtk::CssProvider) {
    let light = glib::EnumValue::from_value(&settings.property_value("gtk-interface-color-scheme"))
        .is_some_and(|(_, scheme)| scheme.nick() == "light");
    if let Some(value) = provider
        .find_property("prefers-color-scheme")
        .and_then(|spec| glib::EnumClass::with_type(spec.value_type()))
        .and_then(|class| class.to_value_by_nick(if light { "light" } else { "dark" }))
    {
        provider.set_property_from_value("prefers-color-scheme", &value);
    }
}

fn style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("ui.css"));
    if let Some(settings) = gtk::Settings::default()
        && settings
            .find_property("gtk-interface-color-scheme")
            .is_some()
    {
        follow_scheme(&settings, &provider);
        let target = provider.clone();
        settings.connect_notify_local(Some("gtk-interface-color-scheme"), move |settings, _| {
            follow_scheme(settings, &target)
        });
    }
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn empty_state(workspace: &Path, entry: &gtk::TextView) -> gtk::Box {
    let empty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    empty.set_valign(gtk::Align::Center);
    empty.set_halign(gtk::Align::Center);
    empty.set_vexpand(true);
    // QUINE's engineered panel: the blueprint grid under an ember rule.
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    panel.add_css_class("blueprint");
    panel.set_width_request(520);
    let hello = label("", "hero");
    hello.set_markup(&format!("Lucy<span foreground=\"{EMBER}\">.</span>"));
    panel.append(&hello);
    let comment = label("", "comment");
    comment.set_markup(&format!(
        "<span foreground=\"{EMBER}\">//</span> ai memory, with a personality"
    ));
    panel.append(&comment);
    let lead = label(
        "I remember what we've worked on across your agents, and I can look at the files here when you ask.",
        "lead",
    );
    lead.set_max_width_chars(52);
    lead.set_margin_top(8);
    panel.append(&lead);
    panel.append(&label("I'm asleep until you send something.", "note"));
    let place = label(&format!("~ working in {}", home(workspace)), "workspace");
    place.add_css_class("mono");
    place.set_halign(gtk::Align::Start);
    place.set_wrap(false);
    place.set_ellipsize(pango::EllipsizeMode::Middle);
    place.set_margin_top(6);
    place.set_tooltip_text(Some(
        "Lucy's tools work in this folder, and only after you approve them.",
    ));
    panel.append(&place);
    let ideas = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(8)
        .row_spacing(8)
        .max_children_per_line(2)
        .homogeneous(false)
        .build();
    ideas.set_margin_top(14);
    for idea in SUGGESTIONS {
        let button = gtk::Button::with_label(idea);
        button.add_css_class("suggestion");
        button.set_halign(gtk::Align::Start);
        button.set_tooltip_text(Some("Put this in the message box"));
        let entry = entry.clone();
        button.connect_clicked(move |_| {
            entry.buffer().set_text(idea);
            entry.grab_focus();
        });
        ideas.insert(&button, -1);
    }
    panel.append(&ideas);
    empty.append(&panel);
    empty
}

struct Settings {
    popover: gtk::Popover,
    provider: gtk::DropDown,
    key_row: gtk::Box,
    key_status: gtk::Label,
    key: gtk::PasswordEntry,
    save_key: gtk::Button,
    model: gtk::DropDown,
    models: gtk::StringList,
    model_status: gtk::Label,
    reload: gtk::Button,
    choose: gtk::Button,
}

/// Model IDs can be long: each shows on one line, shortened in the middle, whole on hover.
fn one_line() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let row = label("", "");
        row.set_wrap(false);
        row.set_ellipsize(pango::EllipsizeMode::Middle);
        row.set_max_width_chars(40);
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            item.set_child(Some(&row));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(row), Some(id)) = (
            item.child().and_downcast::<gtk::Label>(),
            item.item().and_downcast::<gtk::StringObject>(),
        ) {
            row.set_text(&id.string());
            row.set_tooltip_text(Some(&id.string()));
        }
    });
    factory
}

fn settings() -> Settings {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_width_request(380);
    content.append(&label("MODEL AND PROVIDER", "section"));
    let names: Vec<&str> = PROVIDERS.iter().map(|(_, name)| *name).collect();
    let provider = gtk::DropDown::from_strings(&names);
    content.append(&provider);
    // A hosted provider lists its models only with a key, so the key comes first.
    let key_row = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let key_status = label("", "muted");
    key_row.append(&key_status);
    let key_entry = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let key = gtk::PasswordEntry::builder()
        .placeholder_text("API key (saved locally)")
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    let save_key = gtk::Button::with_label("Save key");
    key_entry.append(&key);
    key_entry.append(&save_key);
    key_row.append(&key_entry);
    key_row.set_visible(false);
    content.append(&key_row);
    let models = gtk::StringList::new(&[]);
    let model = gtk::DropDown::builder()
        .model(&models)
        .expression(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        ))
        .enable_search(true)
        .search_match_mode(gtk::StringFilterMatchMode::Substring)
        .factory(&one_line())
        .build();
    content.append(&model);
    let model_status = label("", "muted");
    content.append(&model_status);
    let notice = label(NOTICE, "note");
    notice.set_max_width_chars(48);
    content.append(&notice);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let reload = gtk::Button::with_label("Reload models");
    reload.set_visible(false);
    let choose = gtk::Button::with_label("Use this provider");
    choose.add_css_class("use");
    buttons.append(&reload);
    buttons.append(&choose);
    content.append(&buttons);
    let popover = gtk::Popover::builder().child(&content).build();
    popover.add_css_class("settings");
    Settings {
        popover,
        provider,
        key_row,
        key_status,
        key,
        save_key,
        model,
        models,
        model_status,
        reload,
        choose,
    }
}

pub fn run(workspace: &Path, handoff: Option<Value>) -> Result<()> {
    let client = call(json!({"op":"open"}))?;
    let app = gtk::Application::builder()
        .application_id("dev.nibli.Lucy")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let workspace = workspace.to_path_buf();
    let close_client = client.clone();
    app.connect_startup(|_| style());
    app.connect_activate(move |app| build(app, &client, &workspace, handoff.as_ref()));
    app.set_accels_for_action("win.close", &["<Control>w", "<Control>q"]);
    app.run_with_args::<&str>(&[]);
    // The window is gone. The service stops this conversation's work and ends its open task.
    let mut close = close_client;
    close["op"] = json!("close");
    call_within(close, Duration::from_secs(5))?;
    Ok(())
}

fn build(app: &gtk::Application, client: &Value, workspace: &Path, handoff: Option<&Value>) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Ask Lucy")
        .default_width(860)
        .default_height(880)
        .build();
    window.add_css_class("lucy");

    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let identity = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let name = label("", "name");
    name.set_markup(&format!("Lucy<span foreground=\"{EMBER}\">.</span>"));
    name.set_wrap(false);
    identity.append(&name);
    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pill.add_css_class("pill");
    pill.set_valign(gtk::Align::Center);
    let pill_spinner = gtk::Spinner::new();
    pill_spinner.set_visible(false);
    pill.append(&pill_spinner);
    let pill_dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pill_dot.add_css_class("dot");
    pill_dot.set_valign(gtk::Align::Center);
    pill.append(&pill_dot);
    let pill_label = label("ASLEEP", "");
    pill_label.set_wrap(false);
    pill.append(&pill_label);
    identity.append(&pill);
    header.pack_start(&identity);
    let close = gtk::Button::new();
    let close_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    close_content.append(&gtk::Image::from_icon_name("window-close-symbolic"));
    close_content.append(&gtk::Label::new(Some("Close")));
    close.set_child(Some(&close_content));
    close.add_css_class("close");
    close.set_action_name(Some("win.close"));
    close.set_tooltip_text(Some("Close, and stop everything Lucy is doing (Ctrl+W)"));
    let sleep = gtk::Button::with_label("Sleep");
    sleep.set_tooltip_text(Some(
        "Lucy, sleep: end this conversation and keep the window open",
    ));
    let Settings {
        popover,
        provider,
        key_row,
        key_status,
        key,
        save_key,
        model,
        models,
        model_status,
        reload,
        choose,
    } = settings();
    let model_button = gtk::MenuButton::builder()
        .label("Local")
        .popover(&popover)
        .build();
    header.pack_end(&close);
    header.pack_end(&sleep);
    header.pack_end(&model_button);
    window.set_titlebar(Some(&header));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let banner_label = label("", "");
    banner_label.set_selectable(true);
    banner_label.set_hexpand(true);
    let dismiss = chip("Dismiss");
    dismiss.set_valign(gtk::Align::Start);
    let banner_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    banner_row.add_css_class("banner");
    banner_row.set_margin_start(18);
    banner_row.set_margin_end(18);
    banner_row.set_margin_top(12);
    banner_row.append(&banner_label);
    banner_row.append(&dismiss);
    let banner = gtk::Revealer::builder()
        .child(&banner_row)
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .build();
    body.append(&banner);

    let messages = gtk::Box::new(gtk::Orientation::Vertical, 18);
    messages.set_margin_start(22);
    messages.set_margin_end(22);
    messages.set_margin_top(20);
    messages.set_margin_bottom(20);
    let conversation = gtk::ScrolledWindow::builder()
        .child(&messages)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    body.append(&conversation);

    let task_rows = gtk::Box::new(gtk::Orientation::Vertical, 6);
    task_rows.add_css_class("taskbar");
    task_rows.set_margin_start(18);
    task_rows.set_margin_end(18);
    task_rows.set_margin_top(4);
    let taskbar = gtk::Revealer::builder()
        .child(&task_rows)
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .build();
    body.append(&taskbar);

    let entry = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .accepts_tab(false)
        .top_margin(9)
        .bottom_margin(9)
        .hexpand(true)
        .build();
    let placeholder = label("Ask Lucy…", "placeholder");
    placeholder.set_can_target(false);
    placeholder.set_valign(gtk::Align::Start);
    placeholder.set_margin_top(9);
    let typing = gtk::Overlay::new();
    typing.set_child(Some(&scroll(&entry, 200)));
    typing.add_overlay(&placeholder);
    typing.set_hexpand(true);
    let send = gtk::Button::with_label("Send →");
    send.add_css_class("send");
    send.set_valign(gtk::Align::End);
    send.set_tooltip_text(Some("Send (Enter)"));
    let composer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    composer.add_css_class("composer");
    let prompt = label("❯", "prompt");
    prompt.set_wrap(false);
    prompt.set_valign(gtk::Align::Start);
    prompt.set_margin_top(7);
    composer.append(&prompt);
    composer.append(&typing);
    composer.append(&send);
    let hint = label(
        "enter to send · shift+enter for a new line · ctrl+w to close",
        "hint",
    );
    hint.set_xalign(0.5);
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bottom.set_margin_start(18);
    bottom.set_margin_end(18);
    bottom.set_margin_top(10);
    bottom.set_margin_bottom(12);
    bottom.append(&composer);
    bottom.append(&hint);
    body.append(&bottom);
    window.set_child(Some(&body));

    let empty = empty_state(workspace, &entry);
    messages.append(&empty);
    let thinking = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    thinking.add_css_class("thinking");
    thinking.append(&avatar("small"));
    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    thinking.append(&spinner);
    let thinking_label = label("", "");
    thinking_label.set_valign(gtk::Align::Center);
    thinking.append(&thinking_label);

    let (sender, receiver) = mpsc::channel();
    let state = Rc::new(Window {
        client: client.clone(),
        workspace: workspace.to_path_buf(),
        local_model: super::visible(client["model"].as_str().unwrap_or("local model")),
        generation: Cell::new(0),
        sender,
        busy: Cell::new(None),
        shown_seconds: Cell::new(0),
        pinned: Rc::new(Cell::new(None)),
        pill,
        pill_dot,
        pill_label,
        pill_spinner,
        model_button,
        banner: banner.clone(),
        banner_label,
        messages,
        empty: RefCell::new(Some(empty)),
        thinking,
        thinking_label,
        cards: RefCell::new(Vec::new()),
        taskbar,
        task_rows,
        finished: RefCell::new(Vec::new()),
        entry: entry.clone(),
        send: send.clone(),
        settings: popover.clone(),
        provider: provider.clone(),
        model: model.clone(),
        models,
        model_status,
        reload: reload.clone(),
        key_row,
        key_status,
        key: key.clone(),
        choose: choose.clone(),
        known: RefCell::new(Value::Null),
        catalogs: RefCell::new(BTreeMap::new()),
    });

    // New content scrolls into view; opening a memory list or a raw request does not. The
    // scroll waits for the layout pass that changed the size, or nothing is redrawn.
    let adjustment = conversation.vadjustment();
    let end = |pinned: Rc<Cell<Option<Instant>>>| {
        move |a: &gtk::Adjustment| {
            if pinned.get().is_some_and(|until| Instant::now() < until) {
                let a = a.clone();
                glib::idle_add_local_once(move || a.set_value(a.upper() - a.page_size()));
            }
        }
    };
    adjustment.connect_upper_notify(end(state.pinned.clone()));
    adjustment.connect_page_size_notify(end(state.pinned.clone()));

    let buffer = entry.buffer();
    let shown = placeholder.clone();
    buffer.connect_changed(move |buffer| shown.set_visible(buffer.char_count() == 0));
    if let Some(handoff) = handoff {
        buffer.set_text(&format!(
            "Lucy, help with this context.\nSource: {}\n\n{}",
            handoff["source"].as_str().unwrap_or(""),
            handoff["text"].as_str().unwrap_or("")
        ));
    }
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s = state.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let enter = matches!(
            key,
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
        );
        if enter && !modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
            s.send_text();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    entry.add_controller(keys);

    let s = state.clone();
    send.connect_clicked(move |_| {
        if s.busy.get().is_some() {
            s.thinking_label.set_text("Stopping…");
            s.work(json!({"op":"cancel"}), "control");
            s.entry.grab_focus();
        } else {
            s.send_text();
        }
    });
    let s = state.clone();
    sleep.connect_clicked(move |_| {
        s.work(json!({"op":"sleep"}), "control");
        s.entry.grab_focus();
    });
    dismiss.connect_clicked(move |_| banner.set_reveal_child(false));

    let s = state.clone();
    popover.connect_show(move |_| s.show_provider());
    let s = state.clone();
    provider.connect_selected_notify(move |_| s.show_provider());
    let s = state.clone();
    model.connect_selected_notify(move |_| s.choose.set_sensitive(s.chosen_model().is_some()));
    let s = state.clone();
    key.connect_activate(move |_| s.save_key());
    let s = state.clone();
    save_key.connect_clicked(move |_| s.save_key());
    let s = state.clone();
    reload.connect_clicked(move |_| {
        s.catalogs.borrow_mut().remove(s.current());
        s.show_provider();
    });
    let s = state.clone();
    choose.connect_clicked(move |_| {
        if let Some(model) = s.chosen_model() {
            s.work(
                json!({"op":"provider-select","name":s.current(),"model":model}),
                "select",
            );
        }
    });

    let close = gio::SimpleAction::new("close", None);
    let target = window.downgrade();
    close.connect_activate(move |_, _| {
        if let Some(window) = target.upgrade() {
            window.close();
        }
    });
    window.add_action(&close);

    let s = state.clone();
    let timer = glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(reply) = receiver.try_recv() {
            s.finish(reply);
        }
        s.tick();
        glib::ControlFlow::Continue
    });
    let timer = RefCell::new(Some(timer));
    window.connect_close_request(move |_| {
        if let Some(timer) = timer.borrow_mut().take() {
            timer.remove();
        }
        glib::Propagation::Proceed
    });
    state.status(Status::Asleep);
    state.work(json!({"op":"providers"}), "providers");
    window.present();
    entry.grab_focus();
}
