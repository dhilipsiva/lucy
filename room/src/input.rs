//! Input, detected and queued: keyboard, pointer (mouse and touch), wheel, resize, gamepads,
//! full-screen changes. The browser's events land in a queue the app drains once per frame.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

#[derive(Clone, Debug)]
pub enum InputEvent {
    Draft(String),
    Submit,
    Typing,
    KeyDown {
        key: String,
        repeat: bool,
        modified: bool,
        shift: bool,
    },
    KeyUp {
        key: String,
    },
    PointerDown {
        x: f64,
        y: f64,
        touch: bool,
        id: i32,
    },
    PointerMove {
        x: f64,
        y: f64,
        touch: bool,
        id: i32,
    },
    PointerUp {
        x: f64,
        y: f64,
        touch: bool,
        id: i32,
    },
    PointerCancel {
        id: i32,
    },
    Wheel {
        dy: f64,
    },
    Resize,
    Gamepad {
        connected: bool,
    },
    FullscreenChange,
}
pub type Queue = Rc<RefCell<VecDeque<InputEvent>>>;

type Dispatch = Rc<dyn Fn(InputEvent)>;
thread_local! {static DISPATCH:RefCell<Option<Dispatch>>=RefCell::new(None);}
pub fn direct(dispatch: Dispatch) {
    DISPATCH.with(|d| *d.borrow_mut() = Some(dispatch));
}
fn push(q: &Queue, e: InputEvent) {
    let dispatch = DISPATCH.with(|d| d.borrow().clone());
    if let Some(f) = dispatch {
        f(e);
    } else {
        q.borrow_mut().push_back(e);
    }
}

/// Attach the listeners; they live for the page.
pub fn attach(canvas: &HtmlCanvasElement, q: Queue) {
    let window = web_sys::window().expect("window");
    {
        let q = q.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let key = e.key();
                if e.target()
                    .is_some_and(|t| t.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some())
                {
                    if key == "Enter" && !e.shift_key() && !e.is_composing() {
                        e.prevent_default();
                        push(&q, InputEvent::Submit);
                    }
                    if key == "Escape" {
                        if let Some(t) = e
                            .target()
                            .and_then(|t| t.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                        {
                            let _ = t.blur();
                        }
                    }
                    return;
                }
                let modified = e.meta_key() || e.ctrl_key() || e.alt_key();
                if key == " "
                    || key == "Tab"
                    || key.starts_with("Arrow")
                    || (!modified && matches!(key.as_str(), "PageUp" | "PageDown"))
                {
                    // the app decides; keep the page from scrolling or moving its focus
                    e.prevent_default();
                }
                push(
                    &q,
                    InputEvent::KeyDown {
                        key,
                        repeat: e.repeat(),
                        modified,
                        shift: e.shift_key(),
                    },
                );
            });
        window
            .add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    if let Some(input) = window
        .document()
        .and_then(|d| d.get_element_by_id("composer-input"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        let queue = q.clone();
        let copy = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            push(&queue, InputEvent::Draft(copy.value()))
        });
        input
            .add_event_listener_with_callback("input", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
        let queue = q.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_| push(&queue, InputEvent::Typing));
        input
            .add_event_listener_with_callback("focus", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let q = q.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                push(&q, InputEvent::KeyUp { key: e.key() })
            });
        window
            .add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let q = q.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            push(&q, InputEvent::Resize)
        });
        window
            .add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    if let Some(viewport) = window.visual_viewport() {
        let queue = q.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_| push(&queue, InputEvent::Resize));
        viewport
            .add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let q1 = q.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            push(&q1, InputEvent::Gamepad { connected: true })
        });
        window
            .add_event_listener_with_callback("gamepadconnected", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
        let q2 = q.clone();
        let cb2 = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            push(&q2, InputEvent::Gamepad { connected: false })
        });
        window
            .add_event_listener_with_callback("gamepaddisconnected", cb2.as_ref().unchecked_ref())
            .ok();
        cb2.forget();
    }
    {
        let q = q.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            push(&q, InputEvent::FullscreenChange)
        });
        window
            .document()
            .unwrap()
            .add_event_listener_with_callback("fullscreenchange", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    for (name, kind) in [
        ("pointerdown", 0u8),
        ("pointermove", 1),
        ("pointerup", 2),
        ("pointercancel", 3),
    ] {
        let q = q.clone();
        let c = canvas.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
                let touch = e.pointer_type() == "touch";
                #[allow(clippy::unnecessary_cast)]
                // web-sys unstable APIs use f64; stable uses i32.
                let (x, y, id) = (e.client_x() as f64, e.client_y() as f64, e.pointer_id());
                match kind {
                    0 => {
                        if !touch {
                            c.set_pointer_capture(id).ok();
                        }
                        push(&q, InputEvent::PointerDown { x, y, touch, id });
                    }
                    1 => push(&q, InputEvent::PointerMove { x, y, touch, id }),
                    2 => push(&q, InputEvent::PointerUp { x, y, touch, id }),
                    _ => push(&q, InputEvent::PointerCancel { id }),
                }
            });
        canvas
            .add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let q = q.clone();
        let cb = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            push(&q, InputEvent::Wheel { dy: e.delta_y() });
        });
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_passive(false);
        canvas
            .add_event_listener_with_callback_and_add_event_listener_options(
                "wheel",
                cb.as_ref().unchecked_ref(),
                &opts,
            )
            .ok();
        cb.forget();
    }
}

/// What a connected pad is doing this frame: sticks and newly pressed buttons (by standard index).
#[derive(Clone, Debug, Default)]
pub struct PadState {
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub pressed: Vec<usize>,
    pub any: bool,
}
pub fn poll_gamepads(prev: &mut std::collections::HashMap<u32, Vec<bool>>) -> PadState {
    let mut st = PadState::default();
    let Some(window) = web_sys::window() else {
        return st;
    };
    let Ok(pads) = window.navigator().get_gamepads() else {
        return st;
    };
    for i in 0..pads.length() {
        let v = pads.get(i);
        let Ok(gp) = v.dyn_into::<web_sys::Gamepad>() else {
            continue;
        };
        st.any = true;
        let axes: Vec<f32> = gp
            .axes()
            .iter()
            .map(|a| a.as_f64().unwrap_or(0.0) as f32)
            .collect();
        let dead = |v: f32| if v.abs() < 0.15 { 0.0 } else { v };
        if axes.len() >= 4 {
            st.lx += dead(axes[0]);
            st.ly += dead(axes[1]);
            st.rx += dead(axes[2]);
            st.ry += dead(axes[3]);
        }
        let buttons: Vec<bool> = gp
            .buttons()
            .iter()
            .map(|b| {
                b.dyn_into::<web_sys::GamepadButton>()
                    .map(|b| b.pressed())
                    .unwrap_or(false)
            })
            .collect();
        let was = prev.entry(gp.index()).or_default();
        for (bi, &p) in buttons.iter().enumerate() {
            if p && !was.get(bi).copied().unwrap_or(false) {
                st.pressed.push(bi);
            }
        }
        *was = buttons;
    }
    st
}
