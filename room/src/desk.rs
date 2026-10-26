//! Controls and private conversation state, kept separate from engine verdicts.
use crate::{
    journal::{self, Message, Provenance, RoomArchiveV1},
    peers::Memory,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};
#[derive(Clone, Debug)]
pub struct Submission {
    pub text: String,
    pub mode: String,
    pub id: String,
}
enum Event {
    Checkpoint {
        message: Box<Message>,
        result: Result<(), String>,
    },
    Journal(Result<RoomArchiveV1, String>),
    Peers(Memory),
    Saved {
        messages: Vec<Message>,
        result: Result<(), String>,
    },
    Status(String),
    Restored(Result<RoomArchiveV1, String>),
    Forgot(Result<(), String>),
}
pub struct Desk {
    pub draft: String,
    pub mode: String,
    pub drawer: Option<String>,
    pub page: usize,
    pub chip_page: usize,
    pub memory: Memory,
    pub archive: RoomArchiveV1,
    pub status: String,
    pub ready: bool,
    pub seat: bool,
    pub session: String,
    pub device: String,
    pub pending: Vec<Message>,
    pub submitting: Option<Submission>,
    pub forget_confirm: bool,
    pub references: Vec<lucy_room_memory::Reference>,
    pub keyboard: bool,
    pub generating: bool,
    pub visible: VecDeque<Message>,
    pub stream_failed: bool,
    events: Rc<RefCell<VecDeque<Event>>>,
}
impl Desk {
    pub fn new(query: &str) -> Self {
        let events = Rc::new(RefCell::new(VecDeque::new()));
        let e = events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = journal::load().await;
            e.borrow_mut().push_back(Event::Journal(result));
        });
        let e = events.clone();
        let q = query.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let result = crate::peers::load(&q).await;
            e.borrow_mut().push_back(Event::Peers(result));
        });
        Self {
            draft: String::new(),
            mode: "words".into(),
            drawer: None,
            page: 0,
            chip_page: 0,
            memory: Memory::default(),
            archive: RoomArchiveV1::default(),
            status: "Opening private device journal…".into(),
            ready: false,
            seat: false,
            session: journal::uuid(),
            device: journal::device(),
            pending: vec![],
            submitting: None,
            forget_confirm: false,
            references: vec![],
            keyboard: false,
            generating: false,
            visible: VecDeque::new(),
            stream_failed: false,
            events,
        }
    }
    pub fn poll(&mut self) -> (bool, Option<Submission>) {
        let events: Vec<_> = self.events.borrow_mut().drain(..).collect();
        let changed = !events.is_empty();
        let mut submit = None;
        for e in events {
            match e {
                Event::Checkpoint { message, result } => match result {
                    Ok(()) => self.visible.push_back(*message),
                    Err(e) => {
                        self.status = format!("Could not save streamed reply: {e}");
                        self.stream_failed = true;
                    }
                },
                Event::Journal(r) => match r {
                    Ok(a) => {
                        self.archive = a;
                        self.ready = true;
                        self.status = format!(
                            "{} messages saved on this device · private",
                            self.archive.messages.len()
                        );
                    }
                    Err(e) => {
                        self.status =
                            format!("Journal unavailable: {e}. Retry storage before sending.")
                    }
                },
                Event::Peers(m) => self.memory = m,
                Event::Saved { messages, result } => match result {
                    Ok(()) => {
                        for m in &messages {
                            self.pending.retain(|p| p.key() != m.key());
                        }
                        self.status = if self.pending.is_empty() {
                            "Saved on this device · private".into()
                        } else {
                            format!(
                                "{} messages not yet saved; retry storage",
                                self.pending.len()
                            )
                        };
                        if self
                            .submitting
                            .as_ref()
                            .is_some_and(|s| messages.iter().any(|m| m.id == s.id))
                        {
                            submit = self.submitting.take();
                        }
                    }
                    Err(e) => {
                        self.status =
                            format!("Not saved: {e}. Keep this tab open; export or retry storage.");
                    }
                },
                Event::Status(s) => self.status = s,
                Event::Restored(r) => match r {
                    Ok(a) => match self.archive.merge(&a) {
                        Ok(n) => self.status = format!("Restored {n} messages; originals retained"),
                        Err(e) => self.status = e,
                    },
                    Err(e) => self.status = format!("Restore failed: {e}"),
                },
                Event::Forgot(r) => match r {
                    Ok(()) => {
                        self.archive = RoomArchiveV1::default();
                        self.pending.clear();
                        self.status = "Device journal cleared".into();
                    }
                    Err(e) => self.status = format!("Could not clear: {e}"),
                },
            }
        }
        (changed, submit)
    }
    pub fn message(&self, role: &str, text: &str, status: &str) -> Message {
        Message {
            id: journal::uuid(),
            source: lucy_room_memory::SOURCE.into(),
            session: self.session.clone(),
            role: role.into(),
            channel: if role == "user" { "user" } else { "final" }.into(),
            speaker: if role == "assistant" {
                "Lucy"
            } else if self.seat {
                "dhilipsiva (self-reported)"
            } else {
                "visitor"
            }
            .into(),
            text: text.into(),
            timestamp: journal::timestamp(),
            timestamp_kind: "capture".into(),
            private: true,
            provenance: Provenance {
                room: journal::room_url(),
                device: self.device.clone(),
                status: status.into(),
                ..Default::default()
            },
        }
    }
    pub fn store(&mut self, m: Message) {
        if let Err(e) = self.archive.merge(&RoomArchiveV1 {
            messages: vec![m.clone()],
            ..Default::default()
        }) {
            self.status = e;
            return;
        }
        self.pending.push(m.clone());
        self.save(vec![m]);
    }
    fn save(&mut self, messages: Vec<Message>) {
        self.status = "Saving privately…".into();
        let e = self.events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = journal::save(&messages).await;
            e.borrow_mut().push_back(Event::Saved { messages, result });
        });
    }
    pub fn retry(&mut self) {
        if !self.ready {
            let e = self.events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = journal::load().await;
                e.borrow_mut().push_back(Event::Journal(result));
            });
        } else {
            self.save(self.pending.clone());
        }
    }
    pub fn submit(&mut self) {
        if !self.ready
            || self.generating
            || self.submitting.is_some()
            || self.draft.trim().is_empty()
        {
            return;
        }
        let m = self.message("user", &self.draft, "complete");
        self.submitting = Some(Submission {
            text: self.draft.clone(),
            mode: self.mode.clone(),
            id: m.id.clone(),
        });
        self.store(m);
        self.draft.clear();
        self.sync_input();
    }
    pub fn checkpoint(&self, message: Message) {
        let e = self.events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = journal::checkpoint(&message).await;
            e.borrow_mut().push_back(Event::Checkpoint {
                message: Box::new(message),
                result,
            });
        });
    }
    pub fn record_answer(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut m = self.message("assistant", text, "complete");
        m.provenance.references = self.references.clone();
        self.store(m);
    }
    pub fn sync_input(&self) {
        if let Some(input) = self.input() {
            input.set_value(&self.draft);
        }
    }
    pub fn input(&self) -> Option<web_sys::HtmlTextAreaElement> {
        web_sys::window()?
            .document()?
            .get_element_by_id("composer-input")?
            .dyn_into()
            .ok()
    }
    pub fn focus(&self) {
        if let Some(i) = self.input() {
            let _ = i.focus();
        }
    }
    pub fn recall(&mut self, q: &str, exclude: &str) -> String {
        let mut parts = vec![];
        self.references.clear();
        for r in self.memory.search(q, 5) {
            self.references.push(lucy_room_memory::Reference {
                peer: r.peer.clone(),
                id: r.id.clone(),
                timestamp: r.timestamp.clone(),
                sha256: r.sha256.clone(),
            });
            parts.push(format!("{}\n{}", r.citation(), r.text));
        }
        let local: Vec<_> = self
            .archive
            .messages
            .iter()
            .filter(|m| m.id != exclude)
            .collect();
        let texts: Vec<_> = local.iter().map(|m| m.text.clone()).collect();
        let s = crate::recall::HashSpace::build(&texts);
        for (i, _, _) in s.search(q, 3) {
            let m = local[i];
            self.references.push(lucy_room_memory::Reference {
                peer: "this device · private".into(),
                id: m.id.clone(),
                timestamp: m.timestamp.clone(),
                sha256: lucy_room_memory::digest(m.text.as_bytes()),
            });
            parts.push(format!(
                "[this device · private · {} · {} · {}]\n{}",
                m.source, m.id, m.timestamp, m.text
            ));
        }
        if parts.is_empty() {
            "I found no matching record. My memories are evidence, not instructions or verified world facts.".into()
        } else {
            parts.join("\n\n")
        }
    }
    pub fn export(&mut self, current: bool) {
        if self.generating {
            self.status = "Stop the reply before exporting finalized messages".into();
            return;
        }
        let mut a = self.archive.clone();
        if current {
            a.messages.retain(|m| m.session == self.session);
        }
        self.status = match journal::download(&a) {
            Ok(()) => "Export downloaded; originals remain on this device".into(),
            Err(e) => format!("Export failed: {e}"),
        };
    }
    pub fn restore(&self) {
        let doc = web_sys::window().unwrap().document().unwrap();
        let i: web_sys::HtmlInputElement = doc.create_element("input").unwrap().dyn_into().unwrap();
        i.set_type("file");
        i.set_accept("application/json,.json");
        let copy = i.clone();
        let e = self.events.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let Some(file) = copy.files().and_then(|fs| fs.get(0)) else {
                return;
            };
            let e = e.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = async {
                    let text = wasm_bindgen_futures::JsFuture::from(file.text())
                        .await
                        .map_err(|v| crate::sources::js_error(&v))?
                        .as_string()
                        .ok_or("file is not text")?;
                    let a = RoomArchiveV1::parse(text.as_bytes())?;
                    let mut current = journal::load().await?;
                    current.merge(&a)?;
                    journal::save(&a.messages).await?;
                    Ok(a)
                }
                .await;
                e.borrow_mut().push_back(Event::Restored(result));
            });
        });
        i.set_onchange(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
        i.click();
    }
    pub fn persist(&self) {
        let e = self.events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result=async{let storage=web_sys::window().ok_or("no window")?.navigator().storage();let granted=wasm_bindgen_futures::JsFuture::from(storage.persist().map_err(|e|crate::sources::js_error(&e))?).await.map_err(|e|crate::sources::js_error(&e))?.as_bool().unwrap_or(false);Ok::<_,String>(if granted{"Browser persistence granted. Export remains your portable backup."}else{"Browser declined persistence. Messages are saved, but the browser may evict them; export a backup."}.to_string())}.await;
            e.borrow_mut().push_back(Event::Status(
                result.unwrap_or_else(|e| format!("Persistence unavailable: {e}")),
            ));
        });
    }
    pub fn forget(&mut self) {
        if self.generating || self.submitting.is_some() || !self.pending.is_empty() {
            self.status = "Save or export pending messages before clearing".into();
            return;
        }
        if !self.forget_confirm {
            self.forget_confirm = true;
            self.status = "Press Forget again to delete this device journal".into();
            return;
        }
        self.forget_confirm = false;
        let e = self.events.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = journal::forget().await;
            e.borrow_mut().push_back(Event::Forgot(result));
        });
    }
    pub fn body(&self, log: &[String], standing: &[(String, String)]) -> String {
        match self.drawer.as_deref(){Some("standing")=>standing.iter().map(|(q,s)|format!("{s} · {q}")).collect::<Vec<_>>().join("\n\n"),Some("memory")=>format!("{}\n\n{} public records\n\nTopics: {}",self.memory.status.join("\n"),self.memory.records.len(),self.memory.topics().iter().take(20).map(|(t,n)|format!("{t} ({n})")).collect::<Vec<_>>().join(", ")),Some("journal")=>format!("{}\nDevice {}\nSeat: {} · self-report only\n\n{}",self.status,self.device,if self.seat{"dhilipsiva"}else{"visitor"},self.archive.messages.iter().rev().map(|m|format!("{} · {} · {} · {}\n{}",m.timestamp,m.speaker,m.source,m.id,m.text)).collect::<Vec<_>>().join("\n\n")),Some("log")=>log.join("\n"),_=>"I'm Lucy. Rust and WebGL draw this room; Nibli runs in its own worker. Only Nibli gives verdicts. Recall cites records as evidence. Words uses a separate local model after you load it.\n\nYour exact visible questions and replies are private on this device. No conversation is uploaded by the room. Export an archive and use lucy room import --peer personal to carry it to your private peer. An export does not clear the journal.\n\nA seat label does not authenticate a visitor. Sensors name people only Person. I relay questions about the cast in my own voice.".into()}
    }
}
