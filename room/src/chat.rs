//! The conversation at the centre of my room (roadmap/N9.md): laid out like the desktop's Ask
//! Lucy and dressed in QUINE, built here from Rust on the page's own elements (`index.html`
//! holds only the empty skeleton). The transcript is drawn from the device journal, so it
//! survives a reload; every visitor action becomes a `ChatEvent` for the frame loop, which owns
//! the state. Text from a visitor, a model or a record is only ever set as text, never as markup.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

use crate::cast::{Entity, Group};
use crate::journal::Message;
use crate::script::{display, GREETING, SUGGESTIONS};
use crate::settings::Settings;

#[derive(Clone, Debug)]
pub enum ChatEvent {
    Send(String),
    Stop,
    Subject(Option<String>),
    Filter(String),
    Panel(Option<String>),
    Backend(String),
    Provider(String),
    ProviderModel(String),
    Key(String),
    Accept,
    OllamaBase(String),
    OllamaModel(String),
    LoadWords,
    StopWords,
    Scene,
    Export(bool),
    /// The text of an archive the visitor picked, to add to this device's journal.
    Imported(String),
    Persist,
    Forget,
    Sync,
    /// Back from the scene to the conversation.
    Conversation,
}

/// What the frame loop hands me to draw.
pub struct View<'a> {
    pub messages: &'a [Message],
    pub session: &'a str,
    /// A reply being written: (message id, text so far, who writes it).
    pub streaming: Option<(&'a str, &'a str, &'a str)>,
    pub entities: &'a [Entity],
    pub subject: Option<&'a str>,
    pub pill: String,
    pub busy: bool,
    pub settings: &'a Settings,
    pub words_status: &'a str,
    pub words_ready: bool,
    pub words_busy: bool,
    pub peers: &'a [String],
    pub journal: &'a str,
    /// The captured home kept in this browser, in a few words, if there is one.
    pub capture: Option<&'a str>,
    pub notice: Option<&'a str>,
    pub forget_confirm: bool,
}

pub struct Chat {
    doc: Document,
    root: Element,
    log: Element,
    chips: Element,
    panel: Element,
    notice: Element,
    pill: Element,
    model_button: Element,
    input: HtmlTextAreaElement,
    send: HtmlElement,
    pub events: Rc<RefCell<VecDeque<ChatEvent>>>,
    drawn: Vec<String>,
    live: Option<(String, Element)>,
    chips_key: String,
    panel_key: String,
    pub panel_open: Option<String>,
    filter: String,
    notice_text: String,
}

/// Where my headset check lives; GitHub Pages carries it on every push to main.
const XR_CHECK: &str = "https://dhilipsiva.github.io/lucy/room/xr-check/";
type GarageHook = Rc<dyn Fn(&str)>;
thread_local! {
    static GARAGE: RefCell<Option<GarageHook>> = const { RefCell::new(None) };
}
/// The garage's buttons (roadmap/N8i.md) ask for XR, which needs the visitor's click to still
/// count: the app installs this, and the click listener calls it there and then.
pub fn on_garage(f: GarageHook) {
    GARAGE.with(|g| *g.borrow_mut() = Some(f));
}

fn by_id(doc: &Document, id: &str) -> Option<Element> {
    doc.get_element_by_id(id)
}
fn make(doc: &Document, tag: &str, class: &str, text: Option<&str>) -> Element {
    let e = doc.create_element(tag).expect("element");
    if !class.is_empty() {
        e.set_class_name(class);
    }
    if let Some(t) = text {
        e.set_text_content(Some(t));
    }
    e
}
fn attr(e: &Element, k: &str, v: &str) {
    let _ = e.set_attribute(k, v);
}
fn button(doc: &Document, class: &str, text: &str, action: &str, value: &str) -> Element {
    let b = make(doc, "button", class, Some(text));
    attr(&b, "type", "button");
    attr(&b, "data-action", action);
    if !value.is_empty() {
        attr(&b, "data-value", value);
    }
    b
}
fn hide(e: &Element, hidden: bool) {
    if hidden {
        attr(e, "hidden", "");
    } else {
        let _ = e.remove_attribute("hidden");
    }
}
fn push(q: &Rc<RefCell<VecDeque<ChatEvent>>>, e: ChatEvent) {
    q.borrow_mut().push_back(e);
}
fn listen<E: JsCast + wasm_bindgen::convert::FromWasmAbi + 'static>(
    target: &web_sys::EventTarget,
    kind: &str,
    f: impl FnMut(E) + 'static,
) {
    let cb = Closure::<dyn FnMut(E)>::new(f);
    let _ = target.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref());
    cb.forget();
}
/// The action an event's target asks for: the nearest element with `data-action`.
fn action_of(e: &web_sys::Event) -> Option<(String, String, Element)> {
    let t: Element = e.target()?.dyn_into().ok()?;
    let a = t.closest("[data-action]").ok()??;
    Some((
        a.get_attribute("data-action")?,
        a.get_attribute("data-value").unwrap_or_default(),
        a,
    ))
}

/// Copy one reply's text, as shown, to the clipboard.
fn copy_reply(el: &Element, id: &str) {
    let Some(card) = el.closest(&format!("[data-id=\"{id}\"]")).ok().flatten() else {
        return;
    };
    let Ok(parts) = card.query_selector_all(".ask-text") else {
        return;
    };
    let mut text = vec![];
    for i in 0..parts.length() {
        if let Some(n) = parts.item(i) {
            text.push(n.text_content().unwrap_or_default());
        }
    }
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(&text.join("\n\n"));
    }
    el.set_text_content(Some("Copied"));
}
/// The visitor's file picker for an archive to import; its text comes back as an event.
fn pick_archive(q: &Rc<RefCell<VecDeque<ChatEvent>>>) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(i) = doc
        .create_element("input")
        .map(|e| e.unchecked_into::<HtmlInputElement>())
    else {
        return;
    };
    i.set_type("file");
    i.set_accept("application/json,.json");
    let (copy, q) = (i.clone(), q.clone());
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let Some(file) = copy.files().and_then(|fs| fs.get(0)) else {
            return;
        };
        let q = q.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(v) = wasm_bindgen_futures::JsFuture::from(file.text()).await {
                if let Some(text) = v.as_string() {
                    push(&q, ChatEvent::Imported(text));
                }
            }
        });
    });
    i.set_onchange(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
    i.click();
}

/// A reply's badge: who wrote it, from its provenance.
pub fn badge(m: &Message) -> String {
    let model = m.provenance.model.as_deref().unwrap_or("");
    let who = if model == "scripted" {
        "SCRIPTED".to_string()
    } else if model.starts_with("lucy-") || model == crate::model::MODEL {
        "IN WORDS · my model".to_string()
    } else if let Some((p, name)) = model.split_once(':') {
        format!("IN WORDS · {} · {name}", crate::backends::label(p))
    } else if model.is_empty() {
        "FROM MY ROOM".to_string()
    } else {
        format!("IN WORDS · {model}")
    };
    match m.provenance.status.as_str() {
        "interrupted" => format!("{who} · stopped"),
        "error" => format!("{who} · error"),
        _ => who,
    }
}

impl Chat {
    pub fn new(doc: &Document) -> Option<Chat> {
        let root = by_id(doc, "ask")?;
        let input: HtmlTextAreaElement = by_id(doc, "ask-input")?.dyn_into().ok()?;
        let send: HtmlElement = by_id(doc, "ask-send")?.dyn_into().ok()?;
        let chat = Chat {
            doc: doc.clone(),
            log: by_id(doc, "ask-log")?,
            chips: by_id(doc, "ask-chips")?,
            panel: by_id(doc, "ask-panel")?,
            notice: by_id(doc, "ask-notice")?,
            pill: by_id(doc, "ask-pill")?,
            model_button: by_id(doc, "ask-model")?,
            root,
            input,
            send,
            events: Rc::new(RefCell::new(VecDeque::new())),
            drawn: vec![],
            live: None,
            chips_key: String::new(),
            panel_key: String::new(),
            panel_open: None,
            filter: String::new(),
            notice_text: String::new(),
        };
        chat.wire();
        chat.greet();
        Some(chat)
    }
    fn wire(&self) {
        // the composer: Enter sends, Shift+Enter adds a line, an IME composition is left alone
        {
            let (q, input) = (self.events.clone(), self.input.clone());
            listen::<web_sys::KeyboardEvent>(&self.input, "keydown", move |e| {
                if e.key() == "Enter" && !e.shift_key() && !e.is_composing() {
                    e.prevent_default();
                    let text = input.value();
                    if !text.trim().is_empty() {
                        input.set_value("");
                        push(&q, ChatEvent::Send(text));
                    }
                }
            });
        }
        {
            let (q, input) = (self.events.clone(), self.input.clone());
            listen::<web_sys::Event>(&self.send, "click", move |_| {
                let text = input.value();
                if text.trim().is_empty() {
                    push(&q, ChatEvent::Stop);
                } else {
                    input.set_value("");
                    push(&q, ChatEvent::Send(text));
                }
            });
        }
        // every button in the conversation says what it does in data-action
        let q = self.events.clone();
        listen::<web_sys::Event>(&self.root, "click", move |e| {
            let Some((action, value, el)) = action_of(&e) else {
                return;
            };
            let ev = match action.as_str() {
                "suggest" => ChatEvent::Send(value),
                "subject" => ChatEvent::Subject((!value.is_empty()).then_some(value)),
                "panel" => ChatEvent::Panel((!value.is_empty()).then_some(value)),
                "backend" => ChatEvent::Backend(value),
                "accept" => ChatEvent::Accept,
                "load-words" => ChatEvent::LoadWords,
                "stop-words" => ChatEvent::StopWords,
                "scene" => ChatEvent::Scene,
                "export" => ChatEvent::Export(value == "all"),
                "persist" => ChatEvent::Persist,
                "forget" => ChatEvent::Forget,
                "sync" => ChatEvent::Sync,
                "stop" => ChatEvent::Stop,
                // these two need the visitor's click to still count: they run here, now
                "copy" => {
                    copy_reply(&el, &value);
                    return;
                }
                "import" => {
                    pick_archive(&q);
                    return;
                }
                "garage" => {
                    if let Some(f) = GARAGE.with(|g| g.borrow().clone()) {
                        f(&value);
                    }
                    return;
                }
                _ => return,
            };
            if el.tag_name() == "BUTTON" {
                e.prevent_default();
            }
            push(&q, ev);
        });
        // settings fields report their values when they change
        let q = self.events.clone();
        listen::<web_sys::Event>(&self.root, "change", move |e| {
            let Some(t) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                return;
            };
            let value = if let Some(i) = t.dyn_ref::<HtmlInputElement>() {
                i.value()
            } else if let Some(s) = t.dyn_ref::<web_sys::HtmlSelectElement>() {
                s.value()
            } else {
                return;
            };
            let ev = match t.get_attribute("data-field").as_deref() {
                Some("provider") => ChatEvent::Provider(value),
                Some("provider-model") => ChatEvent::ProviderModel(value),
                Some("key") => ChatEvent::Key(value),
                Some("ollama-base") => ChatEvent::OllamaBase(value),
                Some("ollama-model") => ChatEvent::OllamaModel(value),
                _ => return,
            };
            push(&q, ev);
        });
        if let Some(back) = by_id(&self.doc, "ask-return") {
            let q = self.events.clone();
            listen::<web_sys::Event>(&back, "click", move |e| {
                e.prevent_default();
                push(&q, ChatEvent::Conversation);
            });
        }
        let q = self.events.clone();
        listen::<web_sys::Event>(&self.root, "input", move |e| {
            if let Some(i) = e
                .target()
                .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
            {
                if i.get_attribute("data-field").as_deref() == Some("filter") {
                    push(&q, ChatEvent::Filter(i.value()));
                }
            }
        });
    }
    /// My hello and three suggestions, first in the log and never stored.
    fn greet(&self) {
        let card = make(&self.doc, "article", "ask-empty", None);
        card.append_child(&make(
            &self.doc,
            "p",
            "ask-eyebrow",
            Some("// ai memory, with a personality"),
        ))
        .ok();
        card.append_child(&make(&self.doc, "h1", "ask-hero", Some("Lucy.")))
            .ok();
        card.append_child(&make(&self.doc, "p", "ask-lead", Some(GREETING)))
            .ok();
        let row = make(&self.doc, "div", "ask-suggest", None);
        for s in SUGGESTIONS {
            row.append_child(&button(&self.doc, "ask-pill-btn", s, "suggest", s))
                .ok();
        }
        card.append_child(&row).ok();
        self.log.append_child(&card).ok();
    }
    pub fn show(&self, chat: bool) {
        hide(&self.root, !chat);
        if let Some(body) = self.doc.body() {
            body.set_class_name(if chat { "view-chat" } else { "view-scene" });
        }
        if chat {
            let _ = self.input.focus();
        }
    }
    pub fn focus(&self) {
        let _ = self.input.focus();
    }
    pub fn set_draft(&self, text: &str) {
        self.input.set_value(text);
        let _ = self.input.focus();
    }

    pub fn render(&mut self, v: &View) {
        self.pill.set_text_content(Some(&v.pill));
        let model_label = match v.settings.backend.as_str() {
            "words" => "My model".to_string(),
            "provider" => crate::backends::label(&v.settings.provider),
            "ollama" => "Ollama".to_string(),
            _ => "Scripted".to_string(),
        };
        self.model_button
            .set_text_content(Some(&format!("Model: {model_label} ▾")));
        self.send
            .set_text_content(Some(if v.busy { "Stop ■" } else { "Send →" }));
        attr(
            &self.send,
            "data-busy",
            if v.busy { "true" } else { "false" },
        );
        let placeholder = match v.subject {
            Some(s) => format!("Ask about {}…", display(s)),
            None => "Ask Lucy…".into(),
        };
        attr(&self.input, "placeholder", &placeholder);
        self.render_notice(v.notice);
        self.render_messages(v);
        self.render_chips(v);
        self.render_panel(v);
    }
    fn render_notice(&mut self, notice: Option<&str>) {
        let text = notice.unwrap_or("");
        if text == self.notice_text {
            return;
        }
        self.notice_text = text.to_string();
        self.notice.set_text_content(Some(text));
        hide(&self.notice, text.is_empty());
    }
    fn render_messages(&mut self, v: &View) {
        // the stored transcript: append what is new; a full redraw when it was rewritten
        let keys: Vec<String> = v.messages.iter().map(|m| m.key()).collect();
        let prefix = self.drawn.len() <= keys.len() && self.drawn[..] == keys[..self.drawn.len()];
        if !prefix {
            while let Some(c) = self.log.last_element_child() {
                if c.class_name() == "ask-empty" {
                    break;
                }
                c.remove();
            }
            self.drawn.clear();
            self.live = None;
        }
        let scroll = self.drawn.len() != keys.len() || v.streaming.is_some();
        let mut last_session = v
            .messages
            .get(self.drawn.len().wrapping_sub(1))
            .map(|m| m.session.clone())
            .unwrap_or_default();
        for m in &v.messages[self.drawn.len()..] {
            if let Some((id, el)) = &self.live {
                if *id == m.id {
                    el.remove();
                    self.live = None;
                }
            }
            if m.session != last_session && !last_session.is_empty()
                || (m.session != v.session && last_session.is_empty())
            {
                let label = if m.session == v.session {
                    "This visit".to_string()
                } else {
                    format!("An earlier visit · {}", m.timestamp.get(..10).unwrap_or(""))
                };
                self.log
                    .append_child(&make(&self.doc, "p", "ask-divider", Some(&label)))
                    .ok();
            }
            last_session = m.session.clone();
            let node = self.message_node(m);
            self.log.append_child(&node).ok();
            self.drawn.push(m.key());
        }
        // the reply being written, below the transcript
        match (v.streaming, &self.live) {
            (Some((id, text, who)), Some((live_id, el))) if live_id == id => {
                if let Some(body) = el.query_selector(".ask-text").ok().flatten() {
                    body.set_text_content(Some(if text.is_empty() { "…" } else { text }));
                }
                let _ = who;
            }
            (Some((id, text, who)), _) => {
                if let Some((_, el)) = self.live.take() {
                    el.remove();
                }
                let card = self.lucy_card(
                    &format!("IN WORDS · {who} · writing"),
                    if text.is_empty() { "…" } else { text },
                    &[],
                    "",
                );
                attr(&card, "data-live", "true");
                self.log.append_child(&card).ok();
                self.live = Some((id.to_string(), card));
            }
            (None, Some(_)) => {
                if let Some((_, el)) = self.live.take() {
                    el.remove();
                }
            }
            (None, None) => {}
        }
        if scroll {
            if let Some(h) = self.log.dyn_ref::<HtmlElement>() {
                h.set_scroll_top(h.scroll_height() as f64);
            }
        }
    }
    fn message_node(&self, m: &Message) -> Element {
        if m.role == "user" {
            let row = make(&self.doc, "article", "ask-msg me", None);
            attr(&row, "data-id", &m.id);
            row.append_child(&make(&self.doc, "p", "ask-text", Some(&m.text)))
                .ok();
            return row;
        }
        let refs: Vec<String> = m
            .provenance
            .references
            .iter()
            .map(|r| {
                if r.timestamp.is_empty() {
                    format!("{} · {}", r.peer, r.id)
                } else {
                    format!("{} · {} · {}", r.peer, r.id, r.timestamp)
                }
            })
            .collect();
        self.lucy_card(&badge(m), &m.text, &refs, &m.id)
    }
    fn lucy_card(&self, badge_text: &str, text: &str, refs: &[String], id: &str) -> Element {
        let row = make(&self.doc, "article", "ask-msg lucy", None);
        if !id.is_empty() {
            attr(&row, "data-id", id);
        }
        row.append_child(&make(&self.doc, "span", "ask-avatar", Some("L")))
            .ok();
        let card = make(&self.doc, "div", "ask-card", None);
        let words = badge_text.starts_with("IN WORDS");
        let b = make(
            &self.doc,
            "span",
            if words {
                "ask-badge words"
            } else {
                "ask-badge"
            },
            Some(badge_text),
        );
        if words {
            attr(
                &b,
                "title",
                "Written by a model: words, not a verdict from my engine",
            );
        }
        card.append_child(&b).ok();
        for para in text.split("\n\n") {
            card.append_child(&make(&self.doc, "p", "ask-text", Some(para)))
                .ok();
        }
        let foot = make(&self.doc, "div", "ask-foot", None);
        if !id.is_empty() {
            foot.append_child(&button(&self.doc, "ask-link", "Copy", "copy", id))
                .ok();
        }
        if !refs.is_empty() {
            let d = make(&self.doc, "details", "ask-refs", None);
            d.append_child(&make(
                &self.doc,
                "summary",
                "",
                Some(&format!(
                    "{} {} used",
                    refs.len(),
                    if refs.len() == 1 {
                        "memory"
                    } else {
                        "memories"
                    }
                )),
            ))
            .ok();
            let ul = make(&self.doc, "ul", "", None);
            for r in refs {
                ul.append_child(&make(&self.doc, "li", "", Some(r))).ok();
            }
            d.append_child(&ul).ok();
            foot.append_child(&d).ok();
        }
        card.append_child(&foot).ok();
        row.append_child(&card).ok();
        row
    }
    fn render_chips(&mut self, v: &View) {
        let key = format!(
            "{}|{}|{}",
            v.entities
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(","),
            v.subject.unwrap_or(""),
            self.filter
        );
        if key == self.chips_key {
            return;
        }
        self.chips_key = key;
        while let Some(c) = self.chips.first_element_child() {
            c.remove();
        }
        let mut order: Vec<&Entity> = v
            .entities
            .iter()
            .filter(|e| e.group == Group::Household)
            .collect();
        order.extend(v.entities.iter().filter(|e| e.group == Group::Fiction));
        order.extend(v.entities.iter().filter(|e| e.group == Group::Works));
        if order.len() > 24 {
            let f = make(&self.doc, "input", "ask-filter", None);
            attr(&f, "type", "search");
            attr(&f, "data-field", "filter");
            attr(&f, "placeholder", "Find someone in my memory…");
            attr(&f, "aria-label", "Find someone in my memory");
            if let Some(i) = f.dyn_ref::<HtmlInputElement>() {
                i.set_value(&self.filter);
            }
            self.chips.append_child(&f).ok();
        }
        if let Some(s) = v.subject {
            let b = button(
                &self.doc,
                "ask-chip on",
                &format!("{} ×", display(s)),
                "subject",
                "",
            );
            attr(
                &b,
                "aria-label",
                &format!("Stop asking about {}", display(s)),
            );
            self.chips.append_child(&b).ok();
        }
        let needle = self.filter.to_lowercase();
        for e in order {
            if Some(e.name.as_str()) == v.subject {
                continue;
            }
            let name = display(&e.name);
            if !needle.is_empty()
                && !name.to_lowercase().contains(&needle)
                && !e.name.to_lowercase().contains(&needle)
            {
                continue;
            }
            let b = button(&self.doc, "ask-chip", &name, "subject", &e.name);
            attr(&b, "title", &format!("{} · {}", name, e.kind));
            self.chips.append_child(&b).ok();
        }
    }
    pub fn set_filter(&mut self, f: &str) {
        self.filter = f.to_string();
        self.chips_key.clear();
    }
    fn render_panel(&mut self, v: &View) {
        // only what the open panel shows rebuilds it: never the text a visitor is typing
        let s = v.settings;
        let key = match self.panel_open.as_deref() {
            None => "closed".to_string(),
            Some("model") if s.backend == "words" => format!(
                "model|words|{}|{}|{}",
                v.words_status, v.words_ready, v.words_busy
            ),
            Some("model") => format!("model|{}|{}|{:?}", s.backend, s.provider, s.accepted),
            Some("garage") => "garage".to_string(),
            Some(_) => format!(
                "more|{}|{}|{}",
                v.peers.join("\n"),
                v.journal,
                v.forget_confirm
            ),
        };
        if key == self.panel_key {
            return;
        }
        self.panel_key = key;
        // a rebuild keeps the visitor's place: the field or button they were on keeps focus
        let focused = self
            .doc
            .active_element()
            .filter(|a| self.panel.contains(Some(a)))
            .map(|a| match a.get_attribute("data-field") {
                Some(f) => format!("[data-field=\"{f}\"]"),
                None => format!(
                    "[data-action=\"{}\"][data-value=\"{}\"]",
                    a.get_attribute("data-action").unwrap_or_default(),
                    a.get_attribute("data-value").unwrap_or_default()
                ),
            });
        self.build_panel(v);
        if let Some(e) = focused
            .and_then(|q| self.panel.query_selector(&q).ok().flatten())
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = e.focus();
        }
    }
    fn build_panel(&mut self, v: &View) {
        while let Some(c) = self.panel.first_element_child() {
            c.remove();
        }
        let Some(which) = self.panel_open.clone() else {
            hide(&self.panel, true);
            return;
        };
        hide(&self.panel, false);
        let d = &self.doc;
        let close = button(d, "ask-close", "×", "panel", "");
        attr(&close, "aria-label", "Close");
        self.panel.append_child(&close).ok();
        if which == "garage" {
            self.panel
                .append_child(&make(d, "p", "ask-eyebrow", Some("// a car in my room")))
                .ok();
            // xr-check (roadmap/N8h.md): one tap from here, so nobody types its address in a headset
            let check = make(d, "div", "ask-option", None);
            let link = make(d, "a", "ask-option-btn", Some("Check your headset first"));
            attr(&link, "href", XR_CHECK);
            check.append_child(&link).ok();
            check.append_child(&make(d, "p", "ask-note", Some("A one-minute check of what your headset lets a web page use: floor, walls, depth, camera. Life-size and the miniature depend on it. Nothing of your room is kept; you choose whether to send me the result."))).ok();
            self.panel.append_child(&check).ok();
            for (id, title, note) in [
                ("launch", "Launch strip", "Sit down first. 0–100 km/h in 2.5 s on a straight road: in a headset in VR, or here on this screen. The timing is exact; VR can't push you into the seat, so it's an average 1.13 g you see and hear, not feel."),
                ("life", "Life-size in your room", "Mixed reality on a headset. Point at the floor, pull the trigger, and walk around it."),
                ("mini", "Drive the miniature", "Mixed reality on a headset. A 1:10 car you drive through your home; it stops at the walls and furniture your headset sees. Nothing of your room is kept."),
            ] {
                let row = make(d, "div", "ask-option", None);
                row.append_child(&button(d, "ask-option-btn", title, "garage", id)).ok();
                row.append_child(&make(d, "p", "ask-note", Some(note))).ok();
                self.panel.append_child(&row).ok();
            }
            self.panel
                .append_child(&make(d, "p", "ask-note", Some("The car is a generic SUV I built myself, and its sound is my own: no maker's design or recording.")))
                .ok();
            self.panel
                .append_child(&make(d, "p", "ask-eyebrow", Some("// your home, captured")))
                .ok();
            for (id, title, note) in [
                ("capture", "Capture my home", "Mixed reality on a headset. Walk slowly through your rooms: I record the walls, floor and furniture your headset sees, your path, and every picture its camera gives, as taken. Faces are not blurred here: that happens on your desktop, before I keep anything. Nothing is uploaded; it stays in this browser until you download it. B finishes."),
                ("home", "My captured home", "Look at the capture kept in this browser, its pictures laid on its walls, and line them up."),
                ("home-vr", "Walk it in VR", "The same capture at full size in a headset. Left stick walks, right stick turns, B leaves."),
                ("open", "Open a captured home", "Choose a lucy-home-capture.json file. It replaces the capture kept in this browser. A raw one has its faces blurred here first."),
                ("download", "Download my capture", "One JSON file with the geometry and the pictures, for you to carry to your desktop."),
            ] {
                let row = make(d, "div", "ask-option", None);
                row.append_child(&button(d, "ask-option-btn", title, "garage", id)).ok();
                row.append_child(&make(d, "p", "ask-note", Some(note))).ok();
                self.panel.append_child(&row).ok();
            }
            return;
        }
        if which == "model" {
            self.panel
                .append_child(&make(
                    d,
                    "p",
                    "ask-eyebrow",
                    Some("// who writes my replies"),
                ))
                .ok();
            for (id, title, note) in [
                ("scripted", "Scripted", "Quick, and only from my public memory. The default."),
                ("words", "My model", "Qwen3 0.6B, fine-tuned on my public memory. About 651 MB, downloaded once, runs in your browser."),
                ("provider", "Your provider", "A hosted model, with your own key. Your messages and the public memory I retrieve go to that provider."),
                ("ollama", "Local Ollama", "A model running on your own machine."),
            ] {
                let row = make(d, "div", if v.settings.backend == id { "ask-option on" } else { "ask-option" }, None);
                let b = button(d, "ask-option-btn", title, "backend", id);
                attr(&b, "aria-pressed", if v.settings.backend == id { "true" } else { "false" });
                row.append_child(&b).ok();
                row.append_child(&make(d, "p", "ask-note", Some(note))).ok();
                if id == "words" && v.settings.backend == "words" {
                    row.append_child(&make(d, "p", "ask-note", Some(v.words_status))).ok();
                    if v.words_busy {
                        row.append_child(&button(d, "q-btn", "Stop", "stop-words", "")).ok();
                    } else if !v.words_ready {
                        row.append_child(&button(d, "q-btn primary", "Load my model · 651 MB", "load-words", "")).ok();
                    }
                }
                if id == "provider" && v.settings.backend == "provider" {
                    self.provider_fields(&row, v.settings);
                }
                if id == "ollama" && v.settings.backend == "ollama" {
                    let base = self.field("ollama-base", "Address", &v.settings.ollama_base, "text");
                    row.append_child(&base).ok();
                    let model = self.field("ollama-model", "Model", &v.settings.ollama_model, "text");
                    row.append_child(&model).ok();
                    row.append_child(&make(d, "p", "ask-note", Some("Start Ollama so it accepts this page: OLLAMA_ORIGINS=<this page's origin> ollama serve. If your browser asks whether this page may reach your local network, allow it."))).ok();
                }
                self.panel.append_child(&row).ok();
            }
        } else {
            self.panel
                .append_child(&make(
                    d,
                    "p",
                    "ask-eyebrow",
                    Some("// my memory, and this conversation"),
                ))
                .ok();
            self.panel
                .append_child(&button(d, "q-btn", "Sync from my public peers", "sync", ""))
                .ok();
            let ul = make(d, "ul", "ask-peers", None);
            for line in v.peers {
                ul.append_child(&make(d, "li", "", Some(line))).ok();
            }
            self.panel.append_child(&ul).ok();
            self.panel
                .append_child(&make(d, "p", "ask-note", Some(v.journal)))
                .ok();
            let row = make(d, "div", "ask-row", None);
            row.append_child(&button(d, "q-btn", "Export conversation", "export", "all"))
                .ok();
            row.append_child(&button(d, "q-btn", "Export this visit", "export", "visit"))
                .ok();
            row.append_child(&button(d, "q-btn", "Import…", "import", ""))
                .ok();
            row.append_child(&button(d, "q-btn", "Keep storage", "persist", ""))
                .ok();
            row.append_child(&button(
                d,
                if v.forget_confirm {
                    "q-btn danger"
                } else {
                    "q-btn"
                },
                if v.forget_confirm {
                    "Press again to forget"
                } else {
                    "Forget"
                },
                "forget",
                "",
            ))
            .ok();
            self.panel.append_child(&row).ok();
            if let Some(capture) = v.capture {
                // a captured home is a file of its own (roadmap/N8j.md): say so where he exports
                let row = make(d, "div", "ask-row", None);
                row.append_child(&make(d, "p", "ask-note", Some(&format!("Your captured home ({capture}) is separate: it is not in the conversation file."))))
                    .ok();
                row.append_child(&button(
                    d,
                    "q-btn primary",
                    "Download my capture",
                    "garage",
                    "download",
                ))
                .ok();
                self.panel.append_child(&row).ok();
            }
            self.panel.append_child(&make(d, "p", "ask-note", Some("This conversation stays in this browser. Export gives you a file of the conversation only; Import adds one back. Forget clears the conversation, your settings and any saved key."))).ok();
        }
    }
    fn field(&self, name: &str, label: &str, value: &str, kind: &str) -> Element {
        let l = make(&self.doc, "label", "ask-field", None);
        l.append_child(&make(&self.doc, "span", "", Some(label)))
            .ok();
        let i = make(&self.doc, "input", "", None);
        attr(&i, "type", kind);
        attr(&i, "data-field", name);
        attr(&i, "autocomplete", "off");
        attr(&i, "spellcheck", "false");
        if let Some(input) = i.dyn_ref::<HtmlInputElement>() {
            input.set_value(value);
        }
        l.append_child(&i).ok();
        l
    }
    fn provider_fields(&self, row: &Element, s: &Settings) {
        let d = &self.doc;
        let l = make(d, "label", "ask-field", None);
        l.append_child(&make(d, "span", "", Some("Provider"))).ok();
        let sel = make(d, "select", "", None);
        attr(&sel, "data-field", "provider");
        for p in crate::backends::hosted_presets() {
            let o = make(d, "option", "", Some(&crate::backends::label(&p.name)));
            attr(&o, "value", &p.name);
            if p.name == s.provider {
                attr(&o, "selected", "");
            }
            sel.append_child(&o).ok();
        }
        l.append_child(&sel).ok();
        row.append_child(&l).ok();
        row.append_child(&self.field("provider-model", "Model", &s.provider_model, "text"))
            .ok();
        let has_key = crate::settings::key(&s.provider).is_some();
        let key = self.field(
            "key",
            if has_key {
                "Key (saved in this browser)"
            } else {
                "Key"
            },
            "",
            "password",
        );
        if has_key {
            if let Some(i) = key.query_selector("input").ok().flatten() {
                attr(
                    &i,
                    "placeholder",
                    "•••• saved; paste to replace, clear to remove",
                );
            }
        }
        row.append_child(&key).ok();
        if !s.accepted.contains(&s.provider) {
            row.append_child(&make(d, "p", "ask-note warn", Some(&format!(
                "Before I use {}: your messages, and the public memory I retrieve for them, go to {} under its terms. Your key stays in this browser and is never exported.",
                crate::backends::label(&s.provider),
                crate::backends::label(&s.provider)
            ))))
            .ok();
            row.append_child(&button(
                d,
                "q-btn primary",
                "I understand, use it",
                "accept",
                "",
            ))
            .ok();
        }
    }
    pub fn refresh_panel(&mut self) {
        self.panel_key.clear();
    }
    pub fn open_panel(&mut self, which: Option<String>) {
        self.panel_open = if self.panel_open == which {
            None
        } else {
            which
        };
        self.panel_key.clear();
    }
}
