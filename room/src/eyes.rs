//! Transient observations: anonymous classes, hysteresis, no images or device identity retained.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
struct State {
    hits: Vec<f64>,
    last: f64,
    present: bool,
}
#[derive(Default)]
pub struct Eyes {
    states: BTreeMap<String, State>,
}
pub fn atom(class: &str) -> Option<&'static str> {
    match class.to_lowercase().as_str() {
        "bird" => Some("Bird"),
        "dog" => Some("Dog"),
        "person" | "human" | "face" => Some("Person"),
        _ => None,
    }
}
impl Eyes {
    pub fn ingest(&mut self, line: &str, now: f64) -> Vec<(String, bool)> {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            return vec![];
        };
        let ds = v
            .get("detections")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_else(|| vec![v]);
        let mut seen = BTreeSet::new();
        let mut out = vec![];
        for d in ds {
            let Some(a) = d
                .get("class")
                .or_else(|| d.get("cls"))
                .and_then(|v| v.as_str())
                .and_then(atom)
            else {
                continue;
            };
            let score = d
                .get("score")
                .or_else(|| d.get("confidence"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if !(0.6..=1.0).contains(&score) || !seen.insert(a) {
                continue;
            }
            let s = self.states.entry(a.into()).or_default();
            s.hits.retain(|t| now - t <= 3000.0);
            s.hits.push(now);
            s.last = now;
            if !s.present && s.hits.len() >= 3 {
                s.present = true;
                s.hits.clear();
                out.push((a.into(), true));
            }
        }
        out.extend(self.tick(now));
        out
    }
    pub fn tick(&mut self, now: f64) -> Vec<(String, bool)> {
        let mut out = vec![];
        for (a, s) in &mut self.states {
            if s.present && now - s.last >= 5000.0 {
                s.present = false;
                s.hits.clear();
                out.push((a.clone(), false));
            }
        }
        out
    }
    pub fn simulate(&mut self, a: &str, now: f64) -> Vec<(String, bool)> {
        if a == "clear" {
            return self.tick(now + 5001.0);
        }
        let mut out = vec![];
        for i in 0..3 {
            out.extend(self.ingest(
                &format!("{{\"class\":\"{a}\",\"score\":1}}"),
                now - 200.0 + i as f64 * 100.0,
            ));
        }
        out
    }
}
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};
pub type Lines = Rc<RefCell<VecDeque<Result<String, String>>>>;
fn get(v: &JsValue, k: &str) -> Result<JsValue, String> {
    js_sys::Reflect::get(v, &k.into()).map_err(|e| crate::sources::js_error(&e))
}
fn call(v: &JsValue, k: &str, arg: &JsValue) -> Result<JsValue, String> {
    get(v, k)?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| format!("{k} unavailable"))?
        .call1(v, arg)
        .map_err(|e| crate::sources::js_error(&e))
}
async fn promise(v: JsValue) -> Result<JsValue, String> {
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&v))
        .await
        .map_err(|e| crate::sources::js_error(&e))
}
/// requestDevice is invoked synchronously in the original user gesture.
pub fn connect(lines: Lines) -> Result<(), String> {
    let nav = web_sys::window().ok_or("no window")?.navigator();
    let bt = get(nav.as_ref(), "bluetooth")?;
    let options = js_sys::JSON::parse(
        r#"{"filters":[{"services":["6e400001-b5a3-f393-e0a9-e50e24dcca9e"]}]}"#,
    )
    .map_err(|_| "BLE options")?;
    let pending = call(&bt, "requestDevice", &options)?;
    wasm_bindgen_futures::spawn_local(async move {
        let result = async {
            let device = promise(pending).await?;
            let gatt = get(&device, "gatt")?;
            let server = promise(call(&gatt, "connect", &JsValue::UNDEFINED)?).await?;
            let service = promise(call(
                &server,
                "getPrimaryService",
                &"6e400001-b5a3-f393-e0a9-e50e24dcca9e".into(),
            )?)
            .await?;
            let tx = promise(call(
                &service,
                "getCharacteristic",
                &"6e400003-b5a3-f393-e0a9-e50e24dcca9e".into(),
            )?)
            .await?;
            let target: web_sys::EventTarget = tx.clone().unchecked_into();
            let q = lines.clone();
            let mut buffer: Vec<u8> = vec![];
            let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(t) = e.target() {
                    if let Ok(value) = get(t.as_ref(), "value") {
                        if let Ok(view) = value.dyn_into::<js_sys::DataView>() {
                            for i in 0..view.byte_length() {
                                buffer.push(view.get_uint8(i));
                            }
                            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                                let bytes: Vec<_> = buffer.drain(..=end).collect();
                                match String::from_utf8(bytes) {
                                    Ok(s) => q.borrow_mut().push_back(Ok(s)),
                                    Err(_) => q
                                        .borrow_mut()
                                        .push_back(Err("Invalid UTF-8 from eye".into())),
                                }
                            }
                            if buffer.len() > 8192 {
                                buffer.clear();
                                q.borrow_mut()
                                    .push_back(Err("Eye line exceeded 8 KiB".into()));
                            }
                        }
                    }
                }
            });
            target
                .add_event_listener_with_callback(
                    "characteristicvaluechanged",
                    cb.as_ref().unchecked_ref(),
                )
                .map_err(|e| crate::sources::js_error(&e))?;
            cb.forget();
            promise(call(&tx, "startNotifications", &JsValue::UNDEFINED)?).await?;
            Ok::<_, String>(())
        }
        .await;
        if let Err(e) = result {
            lines.borrow_mut().push_back(Err(e));
        }
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thresholds_anonymity_and_expiry() {
        let mut e = Eyes::default();
        assert_eq!(atom("face"), Some("Person"));
        assert_eq!(atom("Dhilipsiva"), None);
        for t in [0., 100.] {
            assert!(e.ingest(r#"{"class":"bird","score":0.8}"#, t).is_empty());
        }
        assert_eq!(
            e.ingest(r#"{"class":"bird","score":0.8}"#, 200.),
            vec![("Bird".into(), true)]
        );
        assert!(e.tick(5199.).is_empty());
        assert_eq!(e.tick(5200.), vec![("Bird".into(), false)]);
    }
    #[test]
    fn duplicate_detections_are_one_hit() {
        let mut e = Eyes::default();
        assert!(e.ingest(r#"{"detections":[{"class":"face","score":0.9},{"class":"person","score":0.9},{"class":"human","score":0.9}]}"#,0.).is_empty());
        assert!(e
            .ingest(r#"{"class":"person","score":0.59}"#, 100.)
            .is_empty());
    }
}
