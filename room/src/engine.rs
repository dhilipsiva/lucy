//! The main thread's side of the engine (roadmap/N8d.md): a Web Worker when the browser gives
//! one, the mind on this thread when it cannot. The app posts a load and queries and reads the
//! replies from one queue each frame; the glow runs from a query's posting to its answer.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::mind::Mind;
use crate::worker::{handle, FileText, FromWorker, ToWorker};

pub type Replies = Rc<RefCell<VecDeque<FromWorker>>>;

enum Mode {
    /// The worker is created; `Load` waits for its `Ready`.
    Worker {
        worker: web_sys::Worker,
        ready: Rc<Cell<bool>>,
        pending_load: Option<ToWorker>,
    },
    /// The mind on this thread: queries run one per frame, in `pump`.
    Local { mind: Box<Option<Mind>> },
}
pub struct Engine {
    mode: Mode,
    pub replies: Replies,
    queue: VecDeque<(u64, String)>,
    in_flight: Option<(u64, String)>,
    next_id: u64,
    observations: std::collections::BTreeMap<String, bool>,
    created_ms: f64,
    files: Option<Vec<FileText>>,
    /// Frames the room drew between posting a query and taking its answer: the fix made visible.
    frames: u32,
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or_else(js_sys::Date::now)
}

impl Engine {
    /// Try the worker; fall back to this thread when it cannot be made.
    pub fn new(replies: Replies) -> Engine {
        let mode = match Self::spawn(&replies) {
            Some(w) => w,
            None => Mode::Local {
                mind: Box::new(None),
            },
        };
        Engine {
            mode,
            replies,
            queue: VecDeque::new(),
            in_flight: None,
            next_id: 1,
            observations: Default::default(),
            created_ms: now_ms(),
            files: None,
            frames: 0,
        }
    }
    fn spawn(replies: &Replies) -> Option<Mode> {
        let opts = web_sys::WorkerOptions::new();
        opts.set_type(web_sys::WorkerType::Module);
        let worker =
            web_sys::Worker::new_with_options(&crate::sources::worker_url("engine").ok()?, &opts)
                .ok()?;
        let ready = Rc::new(Cell::new(false));
        {
            let replies = replies.clone();
            let ready = ready.clone();
            let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |e: web_sys::MessageEvent| {
                    let Some(text) = e.data().as_string() else {
                        return;
                    };
                    match serde_json::from_str::<FromWorker>(&text) {
                        Ok(FromWorker::Ready) => ready.set(true),
                        Ok(m) => replies.borrow_mut().push_back(m),
                        Err(err) => replies.borrow_mut().push_back(FromWorker::Fault {
                            error: format!("unreadable reply: {err}"),
                        }),
                    }
                },
            );
            worker.set_onmessage(Some(cb.as_ref().unchecked_ref()));
            cb.forget();
        }
        {
            let replies = replies.clone();
            let cb =
                Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |e: web_sys::ErrorEvent| {
                    replies
                        .borrow_mut()
                        .push_back(FromWorker::Fault { error: e.message() });
                });
            worker.set_onerror(Some(cb.as_ref().unchecked_ref()));
            cb.forget();
        }
        Some(Mode::Worker {
            worker,
            ready,
            pending_load: None,
        })
    }
    pub fn in_worker(&self) -> bool {
        matches!(self.mode, Mode::Worker { .. })
    }
    pub fn where_it_runs(&self) -> &'static str {
        if self.in_worker() {
            "in a worker"
        } else {
            "on the main thread"
        }
    }
    pub fn thinking(&self) -> bool {
        self.in_flight.is_some() || !self.queue.is_empty()
    }
    /// Load my files: posted to the worker (once it is ready), or asserted here at once.
    pub fn load(&mut self, files: Vec<FileText>) {
        self.files = Some(files.clone());
        match &mut self.mode {
            Mode::Worker { pending_load, .. } => {
                *pending_load = Some(ToWorker::Load { files });
            }
            Mode::Local { mind } => {
                let r = handle(mind, ToWorker::Load { files }, now_ms);
                self.replies.borrow_mut().push_back(r);
            }
        }
    }
    pub fn see(&mut self, atom: &str, present: bool) {
        self.observations.insert(atom.into(), present);
        let msg = ToWorker::See {
            atom: atom.into(),
            present,
        };
        match &mut self.mode {
            Mode::Worker { worker, .. } => post(worker, &msg),
            Mode::Local { mind } => {
                let r = handle(mind, msg, now_ms);
                self.replies.borrow_mut().push_back(r);
            }
        }
    }
    /// Queue a query; returns its id. The answer comes back through `replies`.
    pub fn ask(&mut self, query: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.queue.push_back((id, query.to_string()));
        id
    }
    /// Give up on the worker and run here; the files, if loaded, are asserted again.
    pub fn fall_back(&mut self) {
        if let Mode::Worker { worker, .. } = &self.mode {
            worker.terminate();
        }
        self.mode = Mode::Local {
            mind: Box::new(None),
        };
        if let Some(request) = self.in_flight.take() {
            self.queue.push_front(request);
        }
        if let Some(files) = self.files.clone() {
            self.load(files);
            for (a, p) in self.observations.clone() {
                self.see(&a, p);
            }
        }
    }
    /// Once a frame: post what waits, or run one query here; a worker silent for 4 s falls back.
    pub fn pump(&mut self) {
        match &mut self.mode {
            Mode::Worker {
                worker,
                ready,
                pending_load,
            } => {
                if !ready.get() {
                    if pending_load.is_some() && now_ms() - self.created_ms > 4_000.0 {
                        self.replies.borrow_mut().push_back(FromWorker::Fault {
                            error: "the worker gave no sign in 4 s".into(),
                        });
                    }
                    return;
                }
                if let Some(load) = pending_load.take() {
                    post(worker, &load);
                }
                if self.in_flight.is_none() {
                    if let Some((id, query)) = self.queue.pop_front() {
                        self.in_flight = Some((id, query.clone()));
                        self.frames = 0;
                        post(worker, &ToWorker::Ask { id, query });
                    }
                }
            }
            Mode::Local { mind } => {
                if let Some((id, query)) = self.queue.pop_front() {
                    let r = handle(mind, ToWorker::Ask { id, query }, now_ms);
                    self.replies.borrow_mut().push_back(r);
                }
            }
        }
    }
    /// The app tells the engine an answer or error for `id` was taken; how many frames it drew
    /// while the engine ran (none on the main thread: it could not draw).
    pub fn answered(&mut self, id: u64) -> u32 {
        if self
            .in_flight
            .as_ref()
            .is_some_and(|(pending, _)| *pending == id)
        {
            self.in_flight = None;
        }
        std::mem::take(&mut self.frames)
    }
    /// The app drew a frame.
    pub fn frame_drawn(&mut self) {
        if self.in_flight.is_some() {
            self.frames += 1;
        }
    }
}
fn post(worker: &web_sys::Worker, m: &ToWorker) {
    if let Ok(s) = serde_json::to_string(m) {
        worker.post_message(&JsValue::from_str(&s)).ok();
    }
}
