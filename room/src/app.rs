//! The room, running: the wake, my mind loading fresh from my origin, standing, the controls,
//! input and the frame loop. Every verdict here comes from `mind`; the glow runs for exactly
//! as long as the engine runs.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

use crate::colors::{self as col, Verdict};
use crate::gl::Gl;
use crate::hud::{AnswerCard, Group, Hud, HudState, Item, ItemKind, Phase, Speech};
use crate::input::{self, InputEvent, Queue};
use crate::mind::{Answer, Mind, CONTROL_QUERY, STANDING_LABELS, STANDING_QUERIES};
use crate::mirrors::Mirrors;
use crate::panel::Action;
use crate::scene::Scene;
use crate::sources::{fetch_text, Candidate, Sources};
use crate::world::World;

const WAKE_MS: f64 = 3600.0;

pub enum Msg {
    Fetched {
        name: String,
        text: String,
        url: String,
        label: String,
    },
    FetchFailed {
        name: String,
        error: String,
    },
    FontsReady,
}
type Msgs = Rc<RefCell<VecDeque<Msg>>>;
type FrameClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

#[derive(Default)]
struct InputFlags {
    keyboard: bool,
    pointer: bool,
    touch: bool,
    gamepad: bool,
}

pub struct App {
    gl: Gl,
    scene: Scene,
    world: World,
    hud: Hud,
    mirrors: Mirrors,
    canvas: HtmlCanvasElement,
    input: Queue,
    msgs: Msgs,
    mind: Option<Mind>,
    texts: HashMap<String, (String, String, String)>,
    fetch_failed: HashSet<String>,
    phase: Phase,
    start_ms: f64,
    last_ms: f64,
    wake_skipped: bool,
    log: Vec<String>,
    standing: Vec<(String, String)>,
    counts: Option<(usize, usize, f64)>,
    speech: Option<Speech>,
    speech_seq: u64,
    answer: Option<AnswerCard>,
    group: String,
    voice: bool,
    hud_hidden: bool,
    fullscreen: bool,
    dock_min: bool,
    pending: Option<(String, String)>,
    thinking_frame: u8,
    version: u64,
    keys: HashSet<String>,
    drag: Option<(f64, f64, bool)>,
    touches: HashMap<i32, (f64, f64, bool)>,
    pads: HashMap<u32, Vec<bool>>,
    flags: InputFlags,
    w: f64,
    h: f64,
    dpr: f64,
    source_label: String,
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}
fn stamp() -> String {
    let s = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    s.get(11..19).unwrap_or("").to_string()
}

/// Create the room and start its loop.
pub fn boot() -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("scene")
        .ok_or("no canvas #scene")?
        .dyn_into()
        .map_err(|_| "#scene is not a canvas")?;
    let dpr = window.device_pixel_ratio().clamp(1.0, 2.0);
    let (w, h) = (
        window
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(1280.0),
        window
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(800.0),
    );
    canvas.set_width((w * dpr) as u32);
    canvas.set_height((h * dpr) as u32);
    let mut gl = Gl::new(&canvas)?;
    gl.resize((w * dpr) as u32, (h * dpr) as u32);
    let mut scene = Scene::new(&gl, crate::world::lights());
    let seed = (js_sys::Date::now() as u64) ^ 0x1D;
    let world = World::build(&gl, &mut scene, seed);
    let hud = Hud::new(dpr);
    let mirrors = Mirrors::new(&document);
    let input: Queue = Rc::new(RefCell::new(VecDeque::new()));
    input::attach(&canvas, input.clone());
    let msgs: Msgs = Rc::new(RefCell::new(VecDeque::new()));
    let t0 = now_ms();
    let mut app = App {
        gl,
        scene,
        world,
        hud,
        mirrors,
        canvas,
        input,
        msgs: msgs.clone(),
        mind: None,
        texts: HashMap::new(),
        fetch_failed: HashSet::new(),
        phase: Phase::Wake,
        start_ms: t0,
        last_ms: t0,
        wake_skipped: false,
        log: Vec::new(),
        standing: Vec::new(),
        counts: None,
        speech: None,
        speech_seq: 0,
        answer: None,
        group: "standing".into(),
        voice: false,
        hud_hidden: false,
        fullscreen: false,
        dock_min: false,
        pending: None,
        thinking_frame: 0,
        version: 1,
        keys: HashSet::new(),
        drag: None,
        touches: HashMap::new(),
        pads: HashMap::new(),
        flags: InputFlags::default(),
        w,
        h,
        dpr,
        source_label: String::new(),
    };
    app.detect_input();
    app.log(&format!(
        "wake: lucy · origin main · nothing leaves this tab · {}",
        window.navigator().user_agent().unwrap_or_default()
    ));
    // my two files, fresh from my origin first
    let query = window.location().search().unwrap_or_default();
    let sources = Sources::from_query(&query);
    for (name, candidates) in [
        ("constitution", sources.constitution.clone()),
        ("memory", sources.memory.clone()),
    ] {
        let msgs = msgs.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut errors = Vec::new();
            for Candidate { url, label } in candidates {
                match fetch_text(&url).await {
                    Ok(text) => {
                        msgs.borrow_mut().push_back(Msg::Fetched {
                            name: name.into(),
                            text,
                            url,
                            label,
                        });
                        return;
                    }
                    Err(e) => errors.push(format!("{label} unavailable at {url} ({e})")),
                }
            }
            msgs.borrow_mut().push_back(Msg::FetchFailed {
                name: name.into(),
                error: errors.join("; "),
            });
        });
    }
    // the fonts: the panels are redrawn once they are in
    {
        let msgs = msgs.clone();
        let fonts = document.fonts();
        wasm_bindgen_futures::spawn_local(async move {
            for f in [
                "500 13px 'IBM Plex Mono'",
                "400 13px 'IBM Plex Mono'",
                "400 15px 'IBM Plex Sans'",
                "600 19px 'Space Grotesk'",
            ] {
                let _ = wasm_bindgen_futures::JsFuture::from(fonts.load(f)).await;
            }
            if let Ok(p) = fonts.ready() {
                let _ = wasm_bindgen_futures::JsFuture::from(p).await;
            }
            msgs.borrow_mut().push_back(Msg::FontsReady);
        });
    }
    let app = Rc::new(RefCell::new(app));
    let f: FrameClosure = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::new(move |t: f64| {
        app.borrow_mut().frame(t);
        if let Some(cb) = f.borrow().as_ref() {
            web_sys::window()
                .unwrap()
                .request_animation_frame(cb.as_ref().unchecked_ref())
                .ok();
        }
    }));
    window
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

impl App {
    fn log(&mut self, line: &str) {
        let full = format!("{} {line}", stamp());
        self.log.push(full.clone());
        if self.log.len() > 400 {
            self.log.remove(0);
        }
        self.mirrors.append_log(&full);
        if self.phase == Phase::Wake {
            self.version += 1;
        }
    }
    fn detect_input(&mut self) {
        let window = web_sys::window().unwrap();
        let mq = |q: &str| {
            window
                .match_media(q)
                .ok()
                .flatten()
                .map(|m| m.matches())
                .unwrap_or(false)
        };
        self.flags.pointer = mq("(pointer: fine)");
        self.flags.touch = mq("(pointer: coarse)");
        self.flags.keyboard = self.flags.pointer;
        self.log(&format!(
            "input: pointer={} touch={} keyboard={}",
            self.flags.pointer,
            self.flags.touch,
            if self.flags.keyboard {
                "assumed"
            } else {
                "unknown"
            }
        ));
        self.version += 1;
    }
    fn legend(&self) -> String {
        let mut rows = Vec::new();
        if self.flags.keyboard {
            rows.push("keyboard: arrows or WASD walk · Q E turn · Shift run · Enter call me · 1 to 6 the standing controls · Home find Lucy · H hud · F full screen · V voice");
        }
        if self.flags.pointer {
            rows.push("mouse: drag to look · wheel to approach · click a control · click Lucy to come closer");
        }
        if self.flags.touch {
            rows.push("touch: one finger looks · two fingers walk · tap a control · tap Lucy to come closer");
        }
        if self.flags.gamepad {
            rows.push("gamepad: left stick walks · right stick looks");
        }
        rows.join("   ·   ")
    }
    fn groups(&self) -> Vec<Group> {
        let mut standing: Vec<Item> = STANDING_QUERIES
            .iter()
            .zip(STANDING_LABELS.iter())
            .map(|(q, l)| Item {
                label: (*l).into(),
                kind: ItemKind::Plain,
                query: Some((*q).into()),
                disabled: false,
            })
            .collect();
        standing.push(Item {
            label: "Something not derivable".into(),
            kind: ItemKind::Plain,
            query: Some(CONTROL_QUERY.into()),
            disabled: false,
        });
        let facts: Vec<Item> = self
            .mind
            .as_ref()
            .map(|m| {
                m.facts
                    .iter()
                    .take(12)
                    .map(|f| Item {
                        label: f.kr.clone(),
                        kind: ItemKind::Plain,
                        query: Some(f.kr.clone()),
                        disabled: false,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut out = vec![Group {
            id: "standing".into(),
            label: "standing".into(),
            items: standing,
        }];
        if !facts.is_empty() {
            out.push(Group {
                id: "facts".into(),
                label: "facts".into(),
                items: facts,
            });
        }
        out
    }
    // ── what I say ──
    fn say(&mut self, text: &str, verdict: Verdict, label: Option<&str>) {
        self.speech_seq += 1;
        self.speech = Some(Speech {
            id: self.speech_seq,
            text: text.into(),
            verdict,
            label: label.map(String::from),
            at_ms: now_ms(),
            streaming: false,
        });
        self.mirrors.set_status(text);
        self.mirrors.set_status_class(match verdict {
            Verdict::True => "bubble t",
            Verdict::False => "bubble f",
            Verdict::Error => "bubble e",
            Verdict::Words => "bubble w",
            Verdict::Recall => "bubble n",
            Verdict::Eyes => "bubble s",
        });
        self.world.lucy.set_color(&mut self.scene, verdict.line());
        if matches!(verdict, Verdict::True | Verdict::False | Verdict::Error) {
            self.world.lucy.flare();
        }
        if self.voice {
            self.speak(text);
        }
        self.version += 1;
    }
    fn speak(&self, text: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(synth) = window.speech_synthesis() else {
            return;
        };
        synth.cancel();
        let short: String = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(600)
            .collect();
        if let Ok(u) = web_sys::SpeechSynthesisUtterance::new_with_text(&short) {
            u.set_rate(1.0);
            u.set_pitch(1.05);
            synth.speak(&u);
        }
    }
    // ── the mind ──
    fn try_build_mind(&mut self) {
        if self.mind.is_some()
            || !(self.texts.contains_key("constitution") && self.texts.contains_key("memory"))
        {
            return;
        }
        let t0 = now_ms();
        let mut mind = Mind::new();
        for name in ["constitution", "memory"] {
            let (text, url, label) = self.texts.get(name).cloned().unwrap();
            let l = mind.assert_file(name, &text, &url, &label);
            self.log(&format!(
                "{name}: {label}, {url}; {}/{} statements asserted, sha256 {}…",
                l.asserted,
                l.statements,
                &l.sha256[..12]
            ));
            for f in &l.failed {
                self.log(&format!("  failed: {f}"));
            }
            if name == "memory" {
                self.source_label = label.clone();
            }
        }
        match mind.assert_standing_fact() {
            Ok(()) => self.log("exist(Lucy, Memory, Loaded). asserted"),
            Err(e) => self.log(&format!("standing fact failed: {e}")),
        }
        mind.ms = now_ms() - t0;
        self.log(&format!("engine ready in {} ms", mind.ms.round()));
        let mut standing = Vec::new();
        for q in STANDING_QUERIES {
            match mind.ask(q, now_ms) {
                Ok(a) => {
                    self.log(&format!(
                        "standing: {q} → {} ({:.1} ms)",
                        a.verdict_word(),
                        a.ms
                    ));
                    standing.push((q.to_string(), a.status.clone()));
                }
                Err(e) => {
                    self.log(&format!("standing: {q} → error {e}"));
                    standing.push((q.to_string(), "ERROR".into()));
                }
            }
        }
        let rules = mind
            .loaded_named("constitution")
            .map(|l| l.asserted)
            .unwrap_or(0);
        let facts = mind.loaded_named("memory").map(|l| l.asserted).unwrap_or(0);
        self.counts = Some((rules, facts, mind.ms));
        self.standing = standing;
        self.mind = Some(mind);
        self.update_mindline();
        self.version += 1;
    }
    fn update_mindline(&self) {
        let ok = !self.standing.is_empty() && self.standing.iter().all(|(_, s)| s == "TRUE");
        if let Some((r, f, ms)) = self.counts {
            self.mirrors.set_mindline(&format!(
                "mind loaded · {r} rules · {f} facts · {} ms · {}",
                ms.round() as i64,
                if ok {
                    "standing holds"
                } else {
                    "standing failed"
                }
            ));
        }
    }
    fn finish_wake(&mut self) {
        if self.phase != Phase::Wake {
            return;
        }
        self.world.wake(&mut self.scene, 1.0);
        self.phase = Phase::Ready;
        let ok = !self.standing.is_empty() && self.standing.iter().all(|(_, s)| s == "TRUE");
        let (r, f, ms) = self.counts.unwrap_or((0, 0, 0.0));
        if ok {
            let line = format!("I'm Lucy. My mind is loaded on this device: {r} rules and {f} facts, {}, in {} ms. Press a control; I answer with a proof.", self.source_label, ms.round() as i64);
            self.say(&line, Verdict::True, Some("AWAKE"));
        } else {
            self.say(
                "I'm Lucy. My memory loaded, but my standing did not hold here. The log says why.",
                Verdict::Error,
                Some("AWAKE"),
            );
        }
        self.log("bubble: awake; the HUD is up");
        self.mirrors.set_answer("Press a control: each issues one fixed Nibli query. A verdict comes with its proof; FALSE means not derivable.");
        self.version += 1;
    }
    fn fail(&mut self, why: &str) {
        self.phase = Phase::Failed;
        self.world.wake(&mut self.scene, 1.0);
        self.say(
            &format!("I'm Lucy. I could not load my mind here: {why}"),
            Verdict::Error,
            Some("ERROR"),
        );
    }
    // ── a query: the glow runs exactly while the engine runs ──
    fn ask_text(&mut self, query: &str, via: &str) {
        if self.mind.is_none() {
            return;
        }
        self.log(&format!("ask ({via}): {query}"));
        self.pending = Some((query.to_string(), via.to_string()));
        self.world.lucy.poke();
    }
    fn run_pending(&mut self) {
        let Some((query, _via)) = self.pending.take() else {
            return;
        };
        let Some(mind) = self.mind.as_ref() else {
            return;
        };
        let result = mind.ask(&query, now_ms);
        match result {
            Ok(a) => self.show_answer(a),
            Err(e) => {
                self.say(
                    &format!("I couldn't read that as Nibli: {e}"),
                    Verdict::Error,
                    None,
                );
                self.answer = Some(AnswerCard {
                    verdict: Verdict::Error,
                    label: None,
                    meta: "nibli · on this device".into(),
                    title: query.clone(),
                    query: String::new(),
                    body: String::new(),
                    why: e.clone(),
                    proof: String::new(),
                    proof_open: false,
                });
                self.mirrors.set_answer(&format!("{query}\n→ {e}"));
                self.log(&format!("query error: {query} — {e}"));
            }
        }
        self.version += 1;
    }
    fn show_answer(&mut self, a: Answer) {
        let verdict = if a.is_true() {
            Verdict::True
        } else if a.is_false() {
            Verdict::False
        } else {
            Verdict::Error
        };
        let spoken = a.spoken();
        self.say(&spoken, verdict, None);
        let label = if a.is_false() && !a.cwa_false {
            Some("FALSE")
        } else {
            None
        };
        self.answer = Some(AnswerCard {
            verdict,
            label: label.map(String::from),
            meta: format!("nibli · on this device · {:.1} ms", a.ms),
            title: a.english.clone(),
            query: a.query.clone(),
            body: String::new(),
            why: a.why.clone(),
            proof: a.proof_text.clone(),
            proof_open: false,
        });
        self.mirrors.set_answer(&format!(
            "{}\n→ {}\n{}\n\n{}\n\n{}",
            a.query,
            a.verdict_word(),
            a.english,
            a.why,
            a.proof_text
        ));
        self.log(&format!(
            "query: {} → {} (engine {:.1} ms; the glow spans the frame it ran in)",
            a.query,
            a.verdict_word(),
            a.ms
        ));
    }
    // ── input ──
    fn call(&mut self, via: &str) {
        let cam = self.world.cam_pos();
        self.world.lucy.call(&self.scene, cam);
        self.world.focus_lucy(&self.scene);
        self.log(&format!("call ({via}): she comes closer"));
        if via != "keyboard" && self.phase == Phase::Ready {
            self.say(
                "Here I am. Press a control; I answer with a proof.",
                Verdict::True,
                Some("LUCY"),
            );
        }
    }
    fn action(&mut self, a: Action) {
        match a {
            Action::Control(i) => {
                let groups = self.groups();
                let group = self.group.clone();
                if let Some(it) = groups
                    .iter()
                    .find(|g| g.id == group)
                    .and_then(|g| g.items.get(i))
                    .cloned()
                {
                    self.log(&format!("control: {}", it.label));
                    if let Some(q) = it.query {
                        self.ask_text(&q, "control");
                    }
                }
            }
            Action::Group(g) => {
                self.group = g;
            }
            Action::ToggleProof => {
                if let Some(a) = &mut self.answer {
                    a.proof_open = !a.proof_open;
                }
            }
            Action::ClearAnswer => {
                self.answer = None;
                self.log("board: cleared");
            }
            Action::ToggleMin => {
                self.dock_min = !self.dock_min;
            }
            Action::Voice => {
                self.voice = !self.voice;
                self.log(&format!("voice: {}", if self.voice { "on" } else { "off" }));
                if self.voice {
                    if let Some(s) = &self.speech {
                        let t = s.text.clone();
                        self.speak(&t);
                    }
                } else if let Some(w) = web_sys::window() {
                    if let Ok(s) = w.speech_synthesis() {
                        s.cancel();
                    }
                }
            }
            Action::Fullscreen => self.toggle_fullscreen(),
            Action::HideHud => {
                self.hud_hidden = true;
            }
            Action::ShowHud => {
                self.hud_hidden = false;
            }
            Action::Find => {
                self.world.focus_lucy(&self.scene);
                self.log("find: turned to Lucy");
            }
            Action::Mind
            | Action::MindTab(_)
            | Action::CloseMind
            | Action::Mode(_)
            | Action::Xr
            | Action::Focus => {}
        }
        self.version += 1;
    }
    fn toggle_fullscreen(&mut self) {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        if doc.fullscreen_element().is_some() {
            doc.exit_fullscreen();
        } else if let Some(el) = doc.document_element() {
            let _ = el.request_fullscreen();
        }
    }
    fn handle(&mut self, ev: InputEvent) {
        let waking = self.phase == Phase::Wake && !self.wake_skipped;
        match ev {
            InputEvent::KeyDown {
                key,
                repeat,
                modified,
            } => {
                if !self.flags.keyboard {
                    self.flags.keyboard = true;
                    self.log("input: keyboard confirmed");
                    self.version += 1;
                }
                if waking && !modified {
                    self.wake_skipped = true;
                    self.log("wake: skipped");
                }
                if repeat {
                    return;
                }
                let k = key.to_lowercase();
                match k.as_str() {
                    "enter" => self.call("keyboard"),
                    "h" => {
                        self.hud_hidden = !self.hud_hidden;
                        self.version += 1;
                    }
                    "f" => self.toggle_fullscreen(),
                    "v" => self.action(Action::Voice),
                    "home" => self.action(Action::Find),
                    "1" | "2" | "3" | "4" | "5" | "6" => {
                        let i = k.parse::<usize>().unwrap_or(1) - 1;
                        if self.group != "standing" {
                            self.group = "standing".into();
                        }
                        self.action(Action::Control(i));
                    }
                    _ => {
                        self.keys.insert(k);
                    }
                }
            }
            InputEvent::KeyUp { key } => {
                self.keys.remove(&key.to_lowercase());
            }
            InputEvent::PointerDown { x, y, touch, id } => {
                if waking {
                    self.wake_skipped = true;
                    self.log("wake: skipped");
                }
                if touch {
                    self.touches.insert(id, (x, y, false));
                    if !self.flags.touch {
                        self.flags.touch = true;
                        self.version += 1;
                    }
                } else {
                    self.drag = Some((x, y, false));
                }
                self.world.lucy.poke();
            }
            InputEvent::PointerMove { x, y, touch, id } => {
                if touch {
                    if let Some(t) = self.touches.get_mut(&id) {
                        let (dx, dy) = (x - t.0, y - t.1);
                        t.0 = x;
                        t.1 = y;
                        if dx.abs() + dy.abs() > 2.0 {
                            t.2 = true;
                        }
                        if self.touches.len() == 1 {
                            self.world.look(-(dx as f32) * 0.005, -(dy as f32) * 0.005);
                        } else {
                            self.world.walk(0.0, -(dy as f32) * 0.01);
                        }
                    }
                } else if let Some(d) = self.drag.as_mut() {
                    let (dx, dy) = (x - d.0, y - d.1);
                    d.0 = x;
                    d.1 = y;
                    if dx.abs() + dy.abs() > 2.0 {
                        d.2 = true;
                    }
                    if d.2 {
                        self.world.look(-(dx as f32) * 0.004, -(dy as f32) * 0.004);
                    }
                } else {
                    let over = self.hud.over_panel(x, y) || {
                        let (o, d) = self.world.ray(x, y, self.w, self.h);
                        self.world.ray_hits_lucy(&self.scene, o, d)
                    };
                    self.canvas
                        .style()
                        .set_property("cursor", if over { "pointer" } else { "grab" })
                        .ok();
                }
            }
            InputEvent::PointerUp { x, y, touch, id } => {
                let moved = if touch {
                    self.touches.remove(&id).map(|t| t.2).unwrap_or(true)
                } else {
                    let m = self.drag.map(|d| d.2).unwrap_or(true);
                    self.drag = None;
                    m
                };
                if !moved {
                    self.click(x, y);
                }
            }
            InputEvent::PointerCancel { id } => {
                self.touches.remove(&id);
                self.drag = None;
            }
            InputEvent::Wheel { dy } => {
                self.world.walk(0.0, -(dy.signum() as f32) * 0.25);
                self.world.lucy.poke();
            }
            InputEvent::Resize => self.resize(),
            InputEvent::Gamepad { connected } => {
                self.flags.gamepad = connected;
                self.log(&format!(
                    "input: gamepad {}",
                    if connected {
                        "connected"
                    } else {
                        "disconnected"
                    }
                ));
                self.version += 1;
            }
            InputEvent::FullscreenChange => {
                self.fullscreen = web_sys::window()
                    .and_then(|w| w.document())
                    .map(|d| d.fullscreen_element().is_some())
                    .unwrap_or(false);
                self.version += 1;
            }
        }
    }
    fn click(&mut self, x: f64, y: f64) {
        match self.hud.hit(x, y) {
            crate::hud::HitResult::Panel(Some(a)) => self.action(a),
            crate::hud::HitResult::Panel(None) => {}
            crate::hud::HitResult::Nothing => {
                let (o, d) = self.world.ray(x, y, self.w, self.h);
                if self.world.ray_hits_lucy(&self.scene, o, d) {
                    self.call("pointer");
                }
            }
        }
    }
    fn resize(&mut self) {
        let Some(window) = web_sys::window() else {
            return;
        };
        self.w = window
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(self.w);
        self.h = window
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(self.h);
        self.dpr = window.device_pixel_ratio().clamp(1.0, 2.0);
        self.canvas.set_width((self.w * self.dpr) as u32);
        self.canvas.set_height((self.h * self.dpr) as u32);
        self.gl
            .resize((self.w * self.dpr) as u32, (self.h * self.dpr) as u32);
        self.version += 1;
    }
    fn poll_pads(&mut self, dt: f32) {
        if !self.flags.gamepad {
            return;
        }
        let st = input::poll_gamepads(&mut self.pads);
        if st.lx != 0.0 || st.ly != 0.0 {
            self.world.walk(st.lx * 1.5 * dt, -st.ly * 1.5 * dt);
            self.world.lucy.poke();
        }
        if st.rx != 0.0 || st.ry != 0.0 {
            self.world.look(-st.rx * 2.0 * dt, -st.ry * 1.5 * dt);
        }
        for b in st.pressed {
            self.log(&format!("pad: button {b} (the pad scheme comes with N8e)"));
        }
    }
    fn poll_keys(&mut self, dt: f32) {
        if self.keys.is_empty() {
            return;
        }
        let sp = if self.keys.contains("shift") {
            2.4
        } else {
            1.2
        } * dt;
        let (mut x, mut z) = (0.0, 0.0);
        if self.keys.contains("w") || self.keys.contains("arrowup") {
            z += 1.0;
        }
        if self.keys.contains("s") || self.keys.contains("arrowdown") {
            z -= 1.0;
        }
        if self.keys.contains("a") || self.keys.contains("arrowleft") {
            x -= 1.0;
        }
        if self.keys.contains("d") || self.keys.contains("arrowright") {
            x += 1.0;
        }
        if x != 0.0 || z != 0.0 {
            self.world.walk(x * sp, z * sp);
            self.world.lucy.poke();
        }
        if self.keys.contains("q") {
            self.world.turn(1.5 * dt);
        }
        if self.keys.contains("e") {
            self.world.turn(-1.5 * dt);
        }
    }
    // ── the frame ──
    pub fn frame(&mut self, now: f64) {
        let dt = ((now - self.last_ms) / 1000.0).clamp(0.0, 0.1) as f32;
        self.last_ms = now;
        let t = ((now - self.start_ms) / 1000.0) as f32;
        // messages from the fetches and the fonts
        let msgs: Vec<Msg> = self.msgs.borrow_mut().drain(..).collect();
        for m in msgs {
            match m {
                Msg::Fetched {
                    name,
                    text,
                    url,
                    label,
                } => {
                    self.texts.insert(name, (text, url, label));
                    self.try_build_mind();
                }
                Msg::FetchFailed { name, error } => {
                    self.log(&format!("{name}: no source could be read ({error})"));
                    self.fetch_failed.insert(name.clone());
                    self.fail(&format!("{name}: no source could be read"));
                }
                Msg::FontsReady => {
                    self.log("fonts: ready");
                    self.version += 1;
                }
            }
        }
        let events: Vec<InputEvent> = self.input.borrow_mut().drain(..).collect();
        for e in events {
            self.handle(e);
        }
        self.poll_keys(dt);
        self.poll_pads(dt);
        // the wake: the animation and the loading, whichever is slower
        if self.phase == Phase::Wake {
            let p = if self.wake_skipped {
                1.0
            } else {
                ((now - self.start_ms) / WAKE_MS).min(1.0) as f32
            };
            if (p - self.world.wake_p).abs() > 1e-4 {
                self.world.wake(&mut self.scene, p);
            }
            if p >= 1.0 && self.mind.is_some() {
                self.finish_wake();
            }
        }
        // a query: one frame with the glow on, then the engine runs
        if self.thinking_frame == 1 {
            self.world.lucy.set_thinking(false);
            self.thinking_frame = 0;
            self.run_pending();
        } else if self.pending.is_some() && self.thinking_frame == 0 {
            self.world.lucy.set_thinking(true);
            self.thinking_frame = 1;
        }
        self.world.step(&mut self.scene, t, dt);
        // render the scene
        let view = self.world.view();
        let proj = World::proj((self.w / self.h) as f32);
        let cam = self.world.cam_pos();
        self.gl
            .clear(col::linear(col::VOID_900).map(|c| c.powf(1.0 / 2.2)));
        self.scene.render(&self.gl, &view, &proj, cam);
        // the HUD
        let stale = self
            .speech
            .as_ref()
            .map(|s| !s.streaming && now - s.at_ms > s.ttl_ms())
            .unwrap_or(false);
        let legend = self.legend();
        let groups = self.groups();
        let wake_animating =
            self.phase == Phase::Wake && !self.wake_skipped && self.world.wake_p < 1.0;
        let wake_progress = if self.phase == Phase::Wake {
            0.5 * self.world.wake_p
                + 0.5
                    * (self.texts.len() as f32 / 2.0)
                    * if self.mind.is_some() { 1.0 } else { 0.8 }
        } else {
            1.0
        };
        let App {
            hud,
            speech,
            answer,
            standing,
            counts,
            phase,
            w,
            h,
            hud_hidden,
            voice,
            fullscreen,
            dock_min,
            group,
            log,
            version,
            ..
        } = self;
        let st = HudState {
            phase: *phase,
            w: *w,
            h: *h,
            counts: *counts,
            standing,
            speech: speech.as_ref(),
            speech_stale: stale,
            answer: answer.as_ref(),
            groups: &groups,
            group,
            legend: &legend,
            hud_hidden: *hud_hidden,
            voice: *voice,
            fullscreen: *fullscreen,
            dock_min: *dock_min,
            wake_lines: log,
            wake_progress,
            wake_animating,
            focus: None,
        };
        let stale_key = if stale { 1u64 << 60 } else { 0 };
        hud.draw(&st, *version ^ stale_key);
        let (hx, hy, visible, on_screen, v) =
            self.world.head_on_screen(&self.scene, self.w, self.h);
        self.hud.place_bubble(hx, hy, visible, self.w, self.h);
        let st2 = HudState {
            phase: self.phase,
            w: self.w,
            h: self.h,
            counts: self.counts,
            standing: &self.standing,
            speech: self.speech.as_ref(),
            speech_stale: stale,
            answer: self.answer.as_ref(),
            groups: &groups,
            group: &self.group,
            legend: &legend,
            hud_hidden: self.hud_hidden,
            voice: self.voice,
            fullscreen: self.fullscreen,
            dock_min: self.dock_min,
            wake_lines: &self.log,
            wake_progress,
            wake_animating,
            focus: None,
        };
        self.hud
            .draw_finder(&st2, v, on_screen || self.phase != Phase::Ready);
        let quad = self.scene.quad_mesh();
        let quad_mesh = &self.scene.meshes[quad];
        self.hud.render(&self.gl, quad_mesh, self.w, self.h);
    }
}
