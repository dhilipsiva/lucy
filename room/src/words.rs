//! Separate model worker; only filtered visible text crosses its message boundary.
use crate::model::{self, Model};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    Load,
    Generate {
        id: String,
        question: String,
        context: String,
        history: String,
    },
    Stop,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Boot,
    Progress {
        phase: String,
        received: usize,
        total: usize,
    },
    Ready,
    Delta {
        id: String,
        text: String,
        generated: usize,
    },
    Done {
        id: String,
        text: String,
        interrupted: bool,
    },
    Error {
        id: Option<String>,
        error: String,
    },
    Fault {
        error: String,
    },
}
pub struct Words {
    worker: Option<web_sys::Worker>,
    pub replies: Rc<RefCell<VecDeque<Reply>>>,
    pub ready: bool,
    pub busy: bool,
    pub status: String,
    last: f64,
}
impl Default for Words {
    fn default() -> Self {
        Self {
            worker: None,
            replies: Default::default(),
            ready: false,
            busy: false,
            status: "Load my words · 651 MB download · on this device".into(),
            last: 0.0,
        }
    }
}
fn post(worker: &web_sys::Worker, msg: &Request) -> Result<(), String> {
    worker
        .post_message(
            &serde_json::to_string(msg)
                .map_err(|e| e.to_string())?
                .into(),
        )
        .map_err(|e| crate::sources::js_error(&e))
}
impl Words {
    pub fn load(&mut self) -> Result<(), String> {
        if self.busy || self.ready {
            return Ok(());
        }
        self.last = js_sys::Date::now();
        self.busy = true;
        self.status = "Starting local model worker…".into();
        if let Some(w) = &self.worker {
            return post(w, &Request::Load);
        }
        let opts = web_sys::WorkerOptions::new();
        opts.set_type(web_sys::WorkerType::Module);
        let worker =
            web_sys::Worker::new_with_options(&crate::sources::worker_url("words")?, &opts)
                .map_err(|e| crate::sources::js_error(&e))?;
        let q = self.replies.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                if let Some(s) = e.data().as_string() {
                    match serde_json::from_str(&s) {
                        Ok(reply) => q.borrow_mut().push_back(reply),
                        Err(e) => q.borrow_mut().push_back(Reply::Fault {
                            error: format!("Model protocol: {e}"),
                        }),
                    }
                }
            });
        worker.set_onmessage(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
        let q = self.replies.clone();
        let cb = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |e: web_sys::ErrorEvent| {
            q.borrow_mut()
                .push_back(Reply::Fault { error: e.message() })
        });
        worker.set_onerror(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
        self.worker = Some(worker);
        Ok(())
    }
    pub fn generate(
        &mut self,
        id: &str,
        question: &str,
        context: &str,
        history: &str,
    ) -> Result<(), String> {
        if !self.ready || self.busy {
            return Err("Load my words before sending a question; your original is saved.".into());
        }
        post(
            self.worker.as_ref().ok_or("model worker unavailable")?,
            &Request::Generate {
                id: id.into(),
                question: question.into(),
                context: context.into(),
                history: history.into(),
            },
        )?;
        self.busy = true;
        self.status = "Reading context…".into();
        self.last = js_sys::Date::now();
        Ok(())
    }
    pub fn stop(&mut self) {
        if let Some(w) = &self.worker {
            let _ = post(w, &Request::Stop);
        }
        self.status = "Stopping…".into();
    }
    pub fn poll(&mut self) -> Vec<Reply> {
        if self.busy && js_sys::Date::now() - self.last > 120_000.0 {
            self.replies.borrow_mut().push_back(Reply::Fault {
                error: "Model worker timed out; retry loading".into(),
            });
        }
        let replies: Vec<_> = self.replies.borrow_mut().drain(..).collect();
        for r in &replies {
            self.last = js_sys::Date::now();
            match r {
                Reply::Boot => {
                    if let Some(w) = &self.worker {
                        let _ = post(w, &Request::Load);
                    }
                }
                Reply::Progress {
                    phase,
                    received,
                    total,
                } => {
                    self.status = if *total > 0 {
                        format!(
                            "{phase} · {:.1} / {:.1} MB",
                            *received as f64 / 1e6,
                            *total as f64 / 1e6
                        )
                    } else {
                        phase.clone()
                    }
                }
                Reply::Ready => {
                    self.ready = true;
                    self.busy = false;
                    self.status = "My words are ready · local Qwen3 0.6B".into();
                }
                Reply::Delta { generated, .. } => {
                    self.status = if *generated == 0 {
                        "Reading context…".into()
                    } else {
                        format!("Writing · {generated} / {} tokens", model::MAX_TOKENS)
                    }
                }
                Reply::Done { interrupted, .. } => {
                    self.busy = false;
                    self.status = if *interrupted {
                        "Stopped; visible reply saved"
                    } else {
                        "Reply complete · local model"
                    }
                    .into();
                }
                Reply::Error { error, .. } => {
                    self.busy = false;
                    self.status = error.clone();
                }
                Reply::Fault { error } => {
                    if let Some(w) = self.worker.take() {
                        w.terminate();
                    }
                    self.busy = false;
                    self.ready = false;
                    self.status = error.clone();
                }
            }
        }
        replies
    }
}
fn send(scope: &web_sys::DedicatedWorkerGlobalScope, reply: Reply) {
    if let Ok(text) = serde_json::to_string(&reply) {
        let _ = scope.post_message(&text.into());
    }
}
fn js(e: JsValue) -> String {
    crate::sources::js_error(&e)
}
const SYSTEM_SHA: &str = "e8b76d73dc3e0eaec8d7a6558a129f2c60eb77657db6bcf2e25432a5960b2986";
async fn asset(
    scope: &web_sys::DedicatedWorkerGlobalScope,
    name: &str,
    hash: &str,
    size: usize,
    cancel: &Cell<bool>,
    signal: &web_sys::AbortSignal,
) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        model::MODEL,
        model::REVISION,
        name
    );
    let cache = match scope.caches() {
        Ok(c) => JsFuture::from(c.open("lucy-room-model-v1"))
            .await
            .ok()
            .and_then(|v| v.dyn_into::<web_sys::Cache>().ok()),
        Err(_) => None,
    };
    if let Some(c) = &cache {
        if let Ok(v) = JsFuture::from(c.match_with_str(&url)).await {
            if let Ok(r) = v.dyn_into::<web_sys::Response>() {
                let data = js_sys::Uint8Array::new(
                    &JsFuture::from(r.array_buffer().map_err(js)?)
                        .await
                        .map_err(js)?,
                )
                .to_vec();
                if lucy_room_memory::digest(&data) == hash {
                    send(
                        scope,
                        Reply::Progress {
                            phase: format!("Cached {name}"),
                            received: data.len(),
                            total: data.len(),
                        },
                    );
                    return Ok(data);
                }
                let _ = JsFuture::from(c.delete_with_str(&url)).await;
            }
        }
    }
    let init = web_sys::RequestInit::new();
    init.set_signal(Some(signal));
    let req = web_sys::Request::new_with_str_and_init(&url, &init).map_err(js)?;
    send(
        scope,
        Reply::Progress {
            phase: format!("Downloading {name}"),
            received: 0,
            total: size,
        },
    );
    let resp: web_sys::Response = JsFuture::from(scope.fetch_with_request(&req))
        .await
        .map_err(js)?
        .dyn_into()
        .map_err(|_| "model response")?;
    if !resp.ok() {
        return Err(format!("Model download HTTP {}", resp.status()));
    }
    let reader: web_sys::ReadableStreamDefaultReader = resp
        .body()
        .ok_or("model body missing")?
        .get_reader()
        .dyn_into()
        .map_err(|_| "model reader")?;
    let mut bytes = Vec::with_capacity(size);
    let mut progress = 0;
    loop {
        if cancel.get() {
            let _ = reader.cancel();
            return Err("Loading stopped".into());
        }
        let part = JsFuture::from(reader.read()).await.map_err(js)?;
        if js_sys::Reflect::get(&part, &"done".into())
            .map_err(js)?
            .as_bool()
            == Some(true)
        {
            break;
        }
        let v = js_sys::Reflect::get(&part, &"value".into()).map_err(js)?;
        let array = js_sys::Uint8Array::new(&v);
        let old = bytes.len();
        bytes.resize(old + array.length() as usize, 0);
        array.copy_to(&mut bytes[old..]);
        if bytes.len() > size {
            return Err("Model size exceeds pinned asset".into());
        }
        if bytes.len() - progress >= 4_000_000 {
            progress = bytes.len();
            send(
                scope,
                Reply::Progress {
                    phase: format!("Downloading {name}"),
                    received: bytes.len(),
                    total: size,
                },
            );
        }
    }
    if bytes.len() != size || lucy_room_memory::digest(&bytes) != hash {
        return Err(format!("Pinned size or SHA-256 mismatch: {name}"));
    }
    if let Some(c) = cache {
        match web_sys::Response::new_with_opt_u8_array(Some(&mut bytes)) {
            Ok(resp) => {
                if JsFuture::from(c.put_with_str(&url, &resp)).await.is_err() {
                    send(
                        scope,
                        Reply::Progress {
                            phase: "Model cache unavailable; loaded bytes remain usable this visit"
                                .into(),
                            received: 0,
                            total: 0,
                        },
                    );
                }
            }
            Err(_) => send(
                scope,
                Reply::Progress {
                    phase: "Could not cache model".into(),
                    received: 0,
                    total: 0,
                },
            ),
        }
    }
    Ok(bytes)
}
#[derive(Default)]
struct Worker {
    model: Option<Model>,
    system: String,
    busy: bool,
    cancel: Rc<Cell<bool>>,
    abort: Option<web_sys::AbortController>,
}
pub fn worker_main() {
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let state: Rc<RefCell<Worker>> = Default::default();
    let out = scope.clone();
    let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
        let Some(text) = e.data().as_string() else {
            return;
        };
        let Ok(request) = serde_json::from_str::<Request>(&text) else {
            return;
        };
        if matches!(request, Request::Stop) {
            let s = state.borrow();
            s.cancel.set(true);
            if let Some(a) = &s.abort {
                a.abort();
            }
            return;
        }
        if state.borrow().busy {
            send(
                &out,
                Reply::Error {
                    id: None,
                    error: "Model is busy".into(),
                },
            );
            return;
        }
        state.borrow_mut().busy = true;
        state.borrow().cancel.set(false);
        let state = state.clone();
        let scope = out.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match request {
                Request::Load => {
                    let cancel = state.borrow().cancel.clone();
                    let result = async {
                        let abort = web_sys::AbortController::new().map_err(js)?;
                        let signal = abort.signal();
                        state.borrow_mut().abort = Some(abort);
                        let gguf = asset(
                            &scope,
                            model::FILE,
                            model::WEIGHTS_SHA,
                            639446912,
                            &cancel,
                            &signal,
                        )
                        .await?;
                        let tokenizer = asset(
                            &scope,
                            "tokenizer.json",
                            model::TOKENIZER_SHA,
                            11422650,
                            &cancel,
                            &signal,
                        )
                        .await?;
                        let system =
                            asset(&scope, "system.txt", SYSTEM_SHA, 425, &cancel, &signal).await?;
                        if cancel.get() {
                            return Err("Loading stopped".into());
                        }
                        send(
                            &scope,
                            Reply::Progress {
                                phase: "Preparing local Qwen3 weights…".into(),
                                received: 0,
                                total: 0,
                            },
                        );
                        let model = Model::load(&gguf, &tokenizer)?;
                        let mut s = state.borrow_mut();
                        s.model = Some(model);
                        s.system = String::from_utf8(system).map_err(|e| e.to_string())?;
                        Ok::<_, String>(())
                    }
                    .await;
                    state.borrow_mut().abort = None;
                    match result {
                        Ok(()) => send(&scope, Reply::Ready),
                        Err(error) => send(&scope, Reply::Error { id: None, error }),
                    }
                }
                Request::Generate {
                    id,
                    question,
                    context,
                    history,
                } => {
                    let cancel = state.borrow().cancel.clone();
                    let mut model = state.borrow_mut().model.take();
                    let system = state.borrow().system.clone();
                    let result = async {
                        let m = model.as_mut().ok_or("Model is not loaded")?;
                        let prompt = m.prompt(&system, &question, &context, &history)?;
                        m.start(&prompt)?;
                        while !m.done() && !cancel.get() {
                            m.step()?;
                            send(
                                &scope,
                                Reply::Delta {
                                    id: id.clone(),
                                    text: m.text().into(),
                                    generated: m.generated(),
                                },
                            );
                            crate::journal::yield_now().await;
                        }
                        Ok::<_, String>((m.text().to_string(), cancel.get()))
                    }
                    .await;
                    state.borrow_mut().model = model;
                    match result {
                        Ok((text, interrupted)) => send(
                            &scope,
                            Reply::Done {
                                id,
                                text,
                                interrupted,
                            },
                        ),
                        Err(error) => send(
                            &scope,
                            Reply::Error {
                                id: Some(id),
                                error,
                            },
                        ),
                    }
                }
                Request::Stop => {}
            }
            state.borrow_mut().busy = false;
        });
    });
    scope.set_onmessage(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
    send(&scope, Reply::Boot);
}
