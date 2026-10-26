//! The engine in a Web Worker (roadmap/N8d.md): the room's thread only draws; the worker holds
//! the mind and answers over messages, so a query no longer freezes the screen. The messages are
//! JSON; `handle` is pure and tested natively on my two files.

use serde::{Deserialize, Serialize};

use crate::mind::{Answer, Fact, Loaded, Mind, STANDING_QUERIES};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileText {
    pub name: String,
    pub text: String,
    pub url: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToWorker {
    Load { files: Vec<FileText> },
    Ask { id: u64, query: String },
    See { atom: String, present: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FromWorker {
    /// The worker's module is up and listening.
    Ready,
    Seen {
        atom: String,
        present: bool,
        fact_count: usize,
    },
    Loaded {
        loaded: Vec<Loaded>,
        standing_fact: Option<String>,
        /// Each standing query, its status, the engine's milliseconds for it.
        standing: Vec<(String, String, f64)>,
        facts: Vec<Fact>,
        /// Loading the files and the standing fact, before the standing queries.
        ms: f64,
    },
    Answer {
        id: u64,
        answer: Answer,
    },
    Error {
        id: u64,
        error: String,
    },
    /// The worker itself failed (a script error); the main thread falls back.
    Fault {
        error: String,
    },
}

/// One message in, one out: the same steps `app` took on the main thread before this slice.
pub fn handle<F: Fn() -> f64 + Copy>(mind: &mut Option<Mind>, msg: ToWorker, now: F) -> FromWorker {
    match msg {
        ToWorker::See { atom, present } => match mind
            .as_mut()
            .ok_or("mind not loaded".to_string())
            .and_then(|m| m.observe(&atom, present))
        {
            Ok(fact_count) => FromWorker::Seen {
                atom,
                present,
                fact_count,
            },
            Err(error) => FromWorker::Error { id: 0, error },
        },
        ToWorker::Load { files } => {
            let t0 = now();
            let mut m = Mind::new();
            let mut loaded = Vec::new();
            for f in &files {
                loaded.push(m.assert_file(&f.name, &f.text, &f.url, &f.label));
            }
            let standing_fact = m.assert_standing_fact().err();
            m.ms = now() - t0;
            let standing = STANDING_QUERIES
                .iter()
                .map(|q| match m.ask(q, now) {
                    Ok(a) => (q.to_string(), a.status, a.ms),
                    Err(e) => (q.to_string(), format!("ERROR {e}"), 0.0),
                })
                .collect();
            let facts = m.facts.clone();
            let ms = m.ms;
            *mind = Some(m);
            FromWorker::Loaded {
                loaded,
                standing_fact,
                standing,
                facts,
                ms,
            }
        }
        ToWorker::Ask { id, query } => match mind {
            Some(m) => match m.ask(&query, now) {
                Ok(answer) => FromWorker::Answer { id, answer },
                Err(error) => FromWorker::Error { id, error },
            },
            None => FromWorker::Error {
                id,
                error: "the mind is not loaded".into(),
            },
        },
    }
}

/// The worker's entry: `worker.js` calls it after the module is instantiated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_main() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    if scope.location().search().contains("role=words") {
        crate::words::worker_main();
        return;
    }
    let mind: Rc<RefCell<Option<Mind>>> = Rc::new(RefCell::new(None));
    let post = {
        let scope = scope.clone();
        move |m: &FromWorker| {
            if let Ok(s) = serde_json::to_string(m) {
                scope.post_message(&JsValue::from_str(&s)).ok();
            }
        }
    };
    let now = || {
        let g: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
        g.performance()
            .map(|p| p.now())
            .unwrap_or_else(js_sys::Date::now)
    };
    let onmessage = {
        let post = post.clone();
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            let Some(text) = e.data().as_string() else {
                return;
            };
            let reply = match serde_json::from_str::<ToWorker>(&text) {
                Ok(msg) => handle(&mut mind.borrow_mut(), msg, now),
                Err(err) => FromWorker::Error {
                    id: 0,
                    error: format!("unreadable message: {err}"),
                },
            };
            post(&reply);
        })
    };
    scope.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
    post(&FromWorker::Ready);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_retract_only_their_own_fact_ids() {
        let mut mind = Some(Mind::new());
        mind.as_mut().unwrap().assert_text("bird(Enma).").unwrap();
        for present in [true, true, false] {
            let r = handle(
                &mut mind,
                ToWorker::See {
                    atom: "Bird".into(),
                    present,
                },
                || 0.0,
            );
            assert!(matches!(r, FromWorker::Seen { .. }));
            assert_eq!(
                mind.as_ref()
                    .unwrap()
                    .ask("observe(Nell, Bird).", || 0.0)
                    .unwrap()
                    .is_true(),
                present
            );
        }
        assert!(mind
            .as_ref()
            .unwrap()
            .ask("bird(Enma).", || 0.0)
            .unwrap()
            .is_true());
        assert!(matches!(
            handle(
                &mut mind,
                ToWorker::See {
                    atom: "Dhilipsiva".into(),
                    present: true
                },
                || 0.0
            ),
            FromWorker::Error { .. }
        ));
    }
    #[test]
    fn the_codec_round_trips() {
        let m = ToWorker::Ask {
            id: 7,
            query: "person(Lucy).".into(),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"kind\":\"ask\""), "{s}");
        assert_eq!(serde_json::from_str::<ToWorker>(&s).unwrap(), m);
        let r = FromWorker::Fault { error: "x".into() };
        let s = serde_json::to_string(&r).unwrap();
        assert!(matches!(
            serde_json::from_str::<FromWorker>(&s).unwrap(),
            FromWorker::Fault { .. }
        ));
    }
    #[test]
    fn ask_before_load_is_an_error_not_a_panic() {
        let mut mind = None;
        let r = handle(
            &mut mind,
            ToWorker::Ask {
                id: 1,
                query: "person(Lucy).".into(),
            },
            || 0.0,
        );
        assert!(matches!(r, FromWorker::Error { id: 1, .. }));
    }
    #[test]
    fn load_then_ask_through_a_minimal_constitution() {
        let mut mind = None;
        let r = handle(
            &mut mind,
            ToWorker::Load {
                files: vec![FileText {
                    name: "constitution".into(),
                    text: "derived_only(\"person\").\nall $e: exist($e, Memory, Loaded) -> person($e).\n".into(),
                    url: "test".into(),
                    label: "test".into(),
                }],
            },
            || 0.0,
        );
        let FromWorker::Loaded {
            loaded, standing, ..
        } = r
        else {
            panic!("not loaded: {r:?}")
        };
        assert_eq!(loaded[0].asserted, 2);
        assert_eq!(standing[0].0, "person(Lucy).");
        assert_eq!(standing[0].1, "TRUE");
        let a = handle(
            &mut mind,
            ToWorker::Ask {
                id: 2,
                query: "person(Lucy)".into(),
            },
            || 0.0,
        );
        assert!(
            matches!(a, FromWorker::Answer { id: 2, ref answer } if answer.is_true()),
            "{a:?}"
        );
    }
}
