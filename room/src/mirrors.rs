//! Hidden text mirrors in the page: my bubble's words, the mind line, the answer board and the
//! log, as plain text for screen readers and for the headless test. They are text, not
//! interface; the interface is drawn in the scene.

use wasm_bindgen::JsCast;
use web_sys::{Document, Element};

pub struct Mirrors {
    status: Element,
    mindline: Element,
    answer: Element,
    log: Element,
    log_text: String,
}
impl Mirrors {
    pub fn new(doc: &Document) -> Mirrors {
        let get = |id: &str, tag: &str, live: bool| -> Element {
            if let Some(e) = doc.get_element_by_id(id) {
                return e;
            }
            let e = doc.create_element(tag).unwrap();
            e.set_id(id);
            e.set_attribute("hidden", "").ok();
            if live {
                e.set_attribute("aria-live", "polite").ok();
            }
            if let Some(body) = doc.body() {
                body.append_child(&e).ok();
            }
            e
        };
        Mirrors {
            status: get("status", "div", true),
            mindline: get("mindline", "div", false),
            answer: get("answer", "div", true),
            log: get("log", "pre", false),
            log_text: String::new(),
        }
    }
    pub fn set_status(&self, text: &str) {
        self.status.set_text_content(Some(text));
    }
    pub fn set_status_class(&self, cls: &str) {
        self.status.set_attribute("class", cls).ok();
    }
    pub fn set_mindline(&self, text: &str) {
        self.mindline.set_text_content(Some(text));
    }
    pub fn set_answer(&self, text: &str) {
        self.answer.set_text_content(Some(text));
    }
    pub fn append_log(&mut self, line: &str) {
        self.log_text.push_str(line);
        self.log_text.push('\n');
        self.log.set_text_content(Some(&self.log_text));
    }
}
/// `console.log` from Rust, for the browser's own log beside the room's.
pub fn console(line: &str) {
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(line));
}
pub fn element_as_html(e: &Element) -> Option<web_sys::HtmlElement> {
    e.clone().dyn_into::<web_sys::HtmlElement>().ok()
}
