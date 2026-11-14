//! The page: buttons, the WebXR session, the 2D camera check, and the table.
//!
//! Plane, mesh, depth and camera access have no web-sys bindings yet, so they are read with
//! `Reflect`. Nothing read from a frame is kept beyond counts, labels and sizes.

use crate::report::{
    Camera2d, CameraDuring, CameraTry, Device, ModeSupport, Probe, Report, Seen, Session, FEATURES,
};
use js_sys::{Array, Function, Object, Reflect};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Document, Element, HtmlCanvasElement, HtmlElement, MediaStream, MediaStreamConstraints,
    MediaStreamTrack, WebGl2RenderingContext, XrFrame, XrReferenceSpace, XrReferenceSpaceType,
    XrRenderStateInit, XrSession, XrSessionInit, XrSessionMode, XrWebGlLayer,
};

/// How long a session runs before it ends itself. Planes arrive two to three seconds in.
const SECONDS: f64 = 8.0;

type Shared = Rc<RefCell<Report>>;
/// N8h.v2: the 2D camera asked for in the same press as the session, and its frame count base.
#[derive(Default)]
struct Gum {
    stream: Option<MediaStream>,
    video: Option<JsValue>,
    base: Option<u32>,
}
type SharedGum = Option<Rc<RefCell<Gum>>>;
type Loop = Rc<RefCell<Option<Closure<dyn FnMut(f64, XrFrame)>>>>;

/// The same reading as room/src/sources.rs `js_error`.
fn js_error(e: &JsValue) -> String {
    e.as_string()
        .or_else(|| {
            Reflect::get(e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
        })
        .filter(|m| !m.is_empty())
        // an OverconstrainedError has no message: its name and the constraint it failed say it
        .or_else(|| {
            let name = get_str(e, "name")?;
            Some(match get_str(e, "constraint") {
                Some(c) if !c.is_empty() => format!("{name} ({c})"),
                _ => name,
            })
        })
        .unwrap_or_else(|| format!("{e:?}"))
}
fn get(v: &JsValue, key: &str) -> JsValue {
    Reflect::get(v, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
fn get_str(v: &JsValue, key: &str) -> Option<String> {
    get(v, key).as_string()
}
fn get_u32(v: &JsValue, key: &str) -> Option<u32> {
    get(v, key).as_f64().map(|f| f as u32)
}
fn present(v: &JsValue) -> bool {
    !(v.is_undefined() || v.is_null())
}
/// Call `obj[name](arg)`, turning a missing method or a throw into words.
fn call1(obj: &JsValue, name: &str, arg: &JsValue) -> Result<JsValue, String> {
    let f: Function = get(obj, name)
        .dyn_into()
        .map_err(|_| format!("{name} is not available"))?;
    f.call1(obj, arg).map_err(|e| js_error(&e))
}
fn call0(obj: &JsValue, name: &str) -> Result<JsValue, String> {
    let f: Function = get(obj, name)
        .dyn_into()
        .map_err(|_| format!("{name} is not available"))?;
    f.call0(obj).map_err(|e| js_error(&e))
}
/// The mean of one frame drawn at 32x24, 0..255. The pixels are read and dropped at once.
fn brightness(video: &JsValue) -> Option<f32> {
    if get(video, "readyState").as_f64().unwrap_or(0.0) < 2.0 {
        return None;
    }
    let canvas: JsValue = doc().create_element("canvas").ok()?.into();
    let _ = Reflect::set(&canvas, &"width".into(), &32.into());
    let _ = Reflect::set(&canvas, &"height".into(), &24.into());
    let ctx = call1(&canvas, "getContext", &"2d".into()).ok()?;
    let draw: Function = get(&ctx, "drawImage").dyn_into().ok()?;
    let args = Array::of5(video, &0.into(), &0.into(), &32.into(), &24.into());
    Reflect::apply(&draw, &ctx, &args).ok()?;
    let read: Function = get(&ctx, "getImageData").dyn_into().ok()?;
    let image = Reflect::apply(
        &read,
        &ctx,
        &Array::of4(&0.into(), &0.into(), &32.into(), &24.into()),
    )
    .ok()?;
    let data = js_sys::Uint8ClampedArray::new(&get(&image, "data")).to_vec();
    let (mut sum, mut n) = (0u64, 0u64);
    for px in data.chunks(4) {
        sum += px[0] as u64 + px[1] as u64 + px[2] as u64;
        n += 3;
    }
    (n > 0).then(|| sum as f32 / n as f32)
}

/// One frame drawn small: the canvas it was drawn on and its light, pixel by pixel.
fn look(video: &JsValue) -> Option<(JsValue, Vec<f64>)> {
    if get(video, "readyState").as_f64().unwrap_or(0.0) < 2.0 {
        return None;
    }
    let (w, h) = (160, 120);
    let canvas: JsValue = doc().create_element("canvas").ok()?.into();
    let _ = Reflect::set(&canvas, &"width".into(), &w.into());
    let _ = Reflect::set(&canvas, &"height".into(), &h.into());
    let ctx = call1(&canvas, "getContext", &"2d".into()).ok()?;
    let draw: Function = get(&ctx, "drawImage").dyn_into().ok()?;
    let args = Array::of5(video, &0.into(), &0.into(), &w.into(), &h.into());
    Reflect::apply(&draw, &ctx, &args).ok()?;
    let read: Function = get(&ctx, "getImageData").dyn_into().ok()?;
    let image = Reflect::apply(
        &read,
        &ctx,
        &Array::of4(&0.into(), &0.into(), &w.into(), &h.into()),
    )
    .ok()?;
    let data = js_sys::Uint8ClampedArray::new(&get(&image, "data")).to_vec();
    let light = data
        .chunks(4)
        .map(|px| (px[0] as f64 + px[1] as f64 + px[2] as f64) / 3.0)
        .collect();
    Some((canvas, light))
}

async fn wait(ms: i32) {
    let promise = js_sys::Promise::new(&mut |ok, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&ok, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// N8h.v3: open one camera by itself, look at it twice, show the second look on the page and
/// keep only numbers. `video` is the constraint: a device id or a facing mode.
async fn try_camera(devices: &web_sys::MediaDevices, asked: &str, video: &JsValue) -> CameraTry {
    let mut t = CameraTry {
        asked: asked.into(),
        ..Default::default()
    };
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(video);
    let stream: MediaStream = match devices.get_user_media_with_constraints(&constraints) {
        Err(e) => {
            t.error = Some(js_error(&e));
            return t;
        }
        Ok(p) => match JsFuture::from(p).await {
            Err(e) => {
                t.error = Some(js_error(&e));
                return t;
            }
            Ok(s) => s.unchecked_into(),
        },
    };
    t.opened = true;
    if let Some(track) = stream.get_video_tracks().iter().next() {
        t.label = get_str(&track, "label").unwrap_or_default();
        if let Ok(settings) = call0(&track, "getSettings") {
            t.width = get_u32(&settings, "width");
            t.height = get_u32(&settings, "height");
            t.facing = get_str(&settings, "facingMode");
        }
    }
    if let Ok(el) = doc().create_element("video") {
        let video: JsValue = el.into();
        let _ = Reflect::set(&video, &"muted".into(), &JsValue::TRUE);
        let _ = Reflect::set(&video, &"playsInline".into(), &JsValue::TRUE);
        let _ = Reflect::set(&video, &"srcObject".into(), &stream);
        let _ = call0(&video, "play");
        let mut first = None;
        for _ in 0..20 {
            wait(150).await;
            first = look(&video);
            if first.is_some() {
                break;
            }
        }
        wait(1500).await;
        let second = look(&video);
        if let Some((canvas, light)) = second.or_else(|| first.clone()) {
            let n = light.len().max(1) as f64;
            let mean = light.iter().sum::<f64>() / n;
            t.brightness = Some(mean);
            t.detail = Some((light.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt());
            if let Some((_, before)) = &first {
                if before.len() == light.len() {
                    t.change = Some(
                        before
                            .iter()
                            .zip(&light)
                            .map(|(a, b)| (a - b).abs())
                            .sum::<f64>()
                            / n,
                    );
                }
            }
            show_thumb(&canvas, &t);
        }
        let _ = Reflect::set(&video, &"srcObject".into(), &JsValue::NULL);
    }
    for track in stream.get_tracks().iter() {
        track.unchecked_into::<MediaStreamTrack>().stop();
    }
    t
}

/// The small picture goes on the page, for his eyes only: it is not in the report.
fn show_thumb(canvas: &JsValue, t: &CameraTry) {
    let (Some(thumbs), Ok(figure), Ok(caption)) = (
        el("thumbs"),
        doc().create_element("figure"),
        doc().create_element("figcaption"),
    ) else {
        return;
    };
    caption.set_text_content(Some(&format!(
        "{} · {}",
        if t.label.is_empty() {
            &t.asked
        } else {
            &t.label
        },
        if t.still() {
            "did not change"
        } else {
            "changed while I looked"
        }
    )));
    if let Some(node) = canvas.dyn_ref::<web_sys::Node>() {
        let _ = figure.append_child(node);
    }
    let _ = figure.append_child(&caption);
    let _ = thumbs.append_child(&figure);
}

fn doc() -> Document {
    web_sys::window().unwrap().document().unwrap()
}
fn el(id: &str) -> Option<Element> {
    doc().get_element_by_id(id)
}
fn set_text(id: &str, text: &str) {
    if let Some(e) = el(id) {
        e.set_text_content(Some(text));
    }
}
fn set_hidden(id: &str, hidden: bool) {
    if let Some(e) = el(id).and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
        e.set_hidden(hidden);
    }
}
fn on_click(id: &str, f: impl FnMut() + 'static) {
    if let Some(e) = el(id) {
        let cb = Closure::<dyn FnMut()>::new(f);
        let _ = e.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let Some(w) = web_sys::window() else {
        return;
    };
    let report: Shared = Rc::new(RefCell::new(Report {
        tool: concat!(
            "lucy xr-check ",
            env!("CARGO_PKG_VERSION"),
            " (roadmap/N8h.md)"
        )
        .into(),
        time: String::from(js_sys::Date::new_0().to_iso_string()),
        user_agent: w.navigator().user_agent().unwrap_or_default(),
        ..Default::default()
    }));
    let xr = w.navigator().xr();
    let has_xr = present(&xr);
    report.borrow_mut().xr_present = has_xr;
    if has_xr {
        for (name, mode) in [
            ("immersive-vr", XrSessionMode::ImmersiveVr),
            ("immersive-ar", XrSessionMode::ImmersiveAr),
        ] {
            report.borrow_mut().modes.push(ModeSupport {
                mode: name.into(),
                supported: None,
                error: None,
            });
            let asked = xr.is_session_supported(mode);
            let r = report.clone();
            spawn_local(async move {
                let answer = JsFuture::from(asked).await;
                if let Some(m) = r.borrow_mut().modes.iter_mut().find(|m| m.mode == name) {
                    match answer {
                        Ok(v) => m.supported = v.as_bool(),
                        Err(e) => m.error = Some(js_error(&e)),
                    }
                }
                render(&r);
            });
        }
    } else {
        set_hidden("check-ar", true);
    }
    let r = report.clone();
    on_click("check-ar", move || check_ar(&r, true, false));
    let r = report.clone();
    on_click("retry-ar", move || check_ar(&r, false, false));
    let r = report.clone();
    // N8h.v2: camera access isn't granted in mixed reality on his headset, so this asks for the
    // session without it, and for the 2D camera in the same press
    on_click("check-ar-camera", move || check_ar(&r, false, true));
    let r = report.clone();
    on_click("check-camera", move || check_camera(&r));
    let r = report.clone();
    on_click("copy", move || copy(&r));
    let r = report.clone();
    on_click("download", move || download(&r));
    render(&report);
}

fn render(r: &Shared) {
    let report = r.borrow();
    let mut html = String::new();
    for row in report.rows() {
        let class = row.status.word().replace(' ', "-");
        html.push_str(&format!(
            "<tr><th scope=\"row\">{}</th><td class=\"s {}\">{}</td><td>{}</td></tr>",
            escape(&row.feature),
            class,
            row.status.word(),
            escape(&row.detail)
        ));
    }
    if let Some(t) = el("rows") {
        t.set_inner_html(&html);
    }
    set_text("json", &report.json());
}

/// requestSession must be called inside the button's activation (WebXR §3.2), so everything
/// up to it is synchronous.
fn check_ar(r: &Shared, with_camera: bool, with_gum: bool) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let xr = w.navigator().xr();
    if !present(&xr) {
        return;
    }
    let requested: Vec<String> = FEATURES
        .iter()
        .filter(|f| with_camera || **f != "camera-access")
        .map(|f| f.to_string())
        .collect();
    let init = XrSessionInit::new();
    let features: Vec<JsValue> = requested.iter().map(|f| JsValue::from_str(f)).collect();
    init.set_optional_features(&features);
    let depth = Object::new();
    let _ = Reflect::set(
        &depth,
        &"usagePreference".into(),
        &Array::of2(&"cpu-optimized".into(), &"gpu-optimized".into()),
    );
    let _ = Reflect::set(
        &depth,
        &"dataFormatPreference".into(),
        &Array::of2(&"luminance-alpha".into(), &"float32".into()),
    );
    let _ = Reflect::set(&init, &"depthSensing".into(), &depth);
    let asked = xr.request_session_with_options(XrSessionMode::ImmersiveAr, &init);
    let index = {
        let mut report = r.borrow_mut();
        report.sessions.push(Session {
            mode: "immersive-ar".into(),
            requested,
            camera_during: with_gum.then(CameraDuring::default),
            ..Default::default()
        });
        report.sessions.len() - 1
    };
    let gum: SharedGum = with_gum.then(|| open_camera(r, index, &w));
    set_text(
        "status",
        "Asking the headset for a mixed-reality session. Look around the room for a few seconds; it ends by itself.",
    );
    render(r);
    let r = r.clone();
    spawn_local(async move {
        match JsFuture::from(asked).await {
            Err(e) => {
                {
                    let mut report = r.borrow_mut();
                    report.sessions[index].started = Some(false);
                    report.sessions[index].error = Some(js_error(&e));
                }
                if with_camera {
                    set_hidden("retry-ar", false);
                    set_text(
                        "status",
                        "The headset refused the session. Try again without camera access.",
                    );
                } else {
                    set_text("status", "The headset refused the session again.");
                }
                render(&r);
            }
            Ok(s) => {
                r.borrow_mut().sessions[index].started = Some(true);
                if let Err(e) = run(&r, index, s.unchecked_into(), gum).await {
                    r.borrow_mut().sessions[index].error = Some(e);
                    set_text("status", "The session started but could not be set up.");
                    render(&r);
                }
            }
        }
    });
}

/// Ask for the 2D camera now, in the same press; it plays into a video element nobody sees.
fn open_camera(r: &Shared, index: usize, w: &web_sys::Window) -> Rc<RefCell<Gum>> {
    let state = Rc::new(RefCell::new(Gum::default()));
    let fail = |r: &Shared, e: String| {
        if let Some(c) = r.borrow_mut().sessions[index].camera_during.as_mut() {
            c.error = Some(e);
        }
    };
    let devices = match w.navigator().media_devices() {
        Ok(d) => d,
        Err(e) => {
            fail(r, js_error(&e));
            return state;
        }
    };
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&JsValue::TRUE);
    let asked = match devices.get_user_media_with_constraints(&constraints) {
        Ok(p) => p,
        Err(e) => {
            fail(r, js_error(&e));
            return state;
        }
    };
    let (r, st) = (r.clone(), state.clone());
    spawn_local(async move {
        match JsFuture::from(asked).await {
            Err(e) => {
                if let Some(c) = r.borrow_mut().sessions[index].camera_during.as_mut() {
                    c.error = Some(js_error(&e));
                }
            }
            Ok(s) => {
                let stream: MediaStream = s.unchecked_into();
                let video: JsValue = match doc().create_element("video") {
                    Ok(v) => v.into(),
                    Err(e) => {
                        if let Some(c) = r.borrow_mut().sessions[index].camera_during.as_mut() {
                            c.error = Some(js_error(&e));
                        }
                        return;
                    }
                };
                let _ = Reflect::set(&video, &"muted".into(), &JsValue::TRUE);
                let _ = Reflect::set(&video, &"playsInline".into(), &JsValue::TRUE);
                let _ = Reflect::set(&video, &"srcObject".into(), &stream);
                let _ = call0(&video, "play");
                if let Some(c) = r.borrow_mut().sessions[index].camera_during.as_mut() {
                    c.opened = true;
                    if let Some(track) = stream.get_video_tracks().iter().next() {
                        if let Ok(settings) = call0(&track, "getSettings") {
                            c.width = get_u32(&settings, "width");
                            c.height = get_u32(&settings, "height");
                        }
                    }
                }
                let mut g = st.borrow_mut();
                g.stream = Some(stream);
                g.video = Some(video);
            }
        }
        render(&r);
    });
    state
}

fn close_camera(gum: &SharedGum) {
    if let Some(g) = gum {
        let mut g = g.borrow_mut();
        if let Some(stream) = g.stream.take() {
            for t in stream.get_tracks().iter() {
                t.unchecked_into::<MediaStreamTrack>().stop();
            }
        }
        if let Some(v) = g.video.take() {
            let _ = Reflect::set(&v, &"srcObject".into(), &JsValue::NULL);
        }
    }
}

async fn run(r: &Shared, index: usize, session: XrSession, gum: SharedGum) -> Result<(), String> {
    {
        let mut report = r.borrow_mut();
        let s = &mut report.sessions[index];
        let enabled = get(&session, "enabledFeatures");
        if present(&enabled) {
            if let Ok(Some(items)) = js_sys::try_iter(&enabled) {
                s.enabled_features = Some(items.flatten().filter_map(|v| v.as_string()).collect());
            }
        }
        s.depth_usage = get_str(&session, "depthUsage");
        s.depth_format = get_str(&session, "depthDataFormat");
    }
    let setup = async {
        let canvas: HtmlCanvasElement = doc()
            .create_element("canvas")
            .map_err(|e| js_error(&e))?
            .unchecked_into();
        let options = Object::new();
        let _ = Reflect::set(&options, &"xrCompatible".into(), &JsValue::TRUE);
        let _ = Reflect::set(&options, &"alpha".into(), &JsValue::TRUE);
        let gl: WebGl2RenderingContext = canvas
            .get_context_with_context_options("webgl2", &options)
            .map_err(|e| js_error(&e))?
            .ok_or("WebGL2 is not available")?
            .unchecked_into();
        JsFuture::from(gl.make_xr_compatible())
            .await
            .map_err(|e| js_error(&e))?;
        let layer = XrWebGlLayer::new_with_web_gl2_rendering_context(&session, &gl)
            .map_err(|e| js_error(&e))?;
        let state = XrRenderStateInit::new();
        state.set_base_layer(Some(&layer));
        session.update_render_state_with_state(&state);
        let (space, name) =
            match JsFuture::from(session.request_reference_space(XrReferenceSpaceType::LocalFloor))
                .await
            {
                Ok(s) => (s, "local-floor"),
                Err(_) => (
                    JsFuture::from(session.request_reference_space(XrReferenceSpaceType::Local))
                        .await
                        .map_err(|e| js_error(&e))?,
                    "local",
                ),
            };
        Ok::<_, String>((gl, layer, space.unchecked_into::<XrReferenceSpace>(), name))
    }
    .await;
    let (gl, layer, space, name) = match setup {
        Ok(v) => v,
        Err(e) => {
            let _ = session.end();
            close_camera(&gum);
            return Err(e);
        }
    };
    r.borrow_mut().sessions[index].reference_space = Some(name.into());
    // GPU depth goes through an XRWebGLBinding, when the browser has one.
    let binding = web_sys::window()
        .map(|w| get(&w, "XRWebGLBinding"))
        .and_then(|c| c.dyn_into::<Function>().ok())
        .and_then(|c| Reflect::construct(&c, &Array::of2(&session, &gl)).ok());

    let frames: Loop = Rc::new(RefCell::new(None));
    let ended = frames.clone();
    let rr = r.clone();
    let closing = gum.clone();
    let end = Closure::<dyn FnMut()>::new(move || {
        ended.borrow_mut().take();
        close_camera(&closing);
        set_text(
            "status",
            "Done. The table below is what your headset granted; Copy or Download sends it to me.",
        );
        render(&rr);
    });
    session.set_onend(Some(end.as_ref().unchecked_ref()));
    end.forget();

    let begun = Rc::new(Cell::new(None::<f64>));
    let next = frames.clone();
    let current = session.clone();
    let rr = r.clone();
    *frames.borrow_mut() = Some(Closure::new(move |now: f64, frame: XrFrame| {
        let first = begun.get().unwrap_or(now);
        begun.set(Some(first));
        gl.bind_framebuffer(
            WebGl2RenderingContext::FRAMEBUFFER,
            layer.framebuffer().as_ref(),
        );
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);
        let seconds = (now - first) / 1000.0;
        {
            let mut report = rr.borrow_mut();
            let s = &mut report.sessions[index];
            s.frames += 1;
            s.seconds = (seconds * 10.0).round() / 10.0;
            seen(&frame, "detectedPlanes", false, &mut s.planes);
            seen(&frame, "detectedMeshes", true, &mut s.meshes);
            // N8h.v2: count the 2D camera's frames while the session runs, and measure one
            if let (Some(g), Some(c)) = (&gum, s.camera_during.as_mut()) {
                if s.frames % 30 == 20 {
                    let mut g = g.borrow_mut();
                    if let Some(video) = g.video.clone() {
                        if let Ok(q) = call0(&video, "getVideoPlaybackQuality") {
                            let total = get_u32(&q, "totalVideoFrames").unwrap_or(0);
                            let base = *g.base.get_or_insert(total);
                            c.frames_during = total.saturating_sub(base);
                        }
                        if seconds >= 3.0 && c.mean_brightness.is_none() {
                            c.mean_brightness = brightness(&video);
                        }
                    }
                }
            }
            // Depth and the camera are asked about every half second until they answer.
            if s.frames % 30 == 10 {
                if let Some(view) = frame
                    .get_viewer_pose(&space)
                    .map(|p| p.views().get(0))
                    .filter(|v| present(v))
                {
                    if !s.depth_cpu.ok {
                        try_depth(
                            &mut s.depth_cpu,
                            call1(&frame, "getDepthInformation", &view),
                        );
                    }
                    if !s.depth_gpu.ok {
                        let answer = match &binding {
                            Some(b) => call1(b, "getDepthInformation", &view),
                            None => Err("XRWebGLBinding is not available".into()),
                        };
                        try_depth(&mut s.depth_gpu, answer);
                    }
                    let camera = get(&view, "camera");
                    s.view_camera.tried = true;
                    if present(&camera) {
                        s.view_camera.ok = true;
                        s.view_camera.width = get_u32(&camera, "width");
                        s.view_camera.height = get_u32(&camera, "height");
                    }
                }
            }
        }
        if seconds >= SECONDS {
            let _ = current.end();
        } else if let Some(cb) = next.borrow().as_ref() {
            current.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    if let Some(cb) = frames.borrow().as_ref() {
        session.request_animation_frame(cb.as_ref().unchecked_ref());
    }
    Ok(())
}

/// Count what a frame's plane or mesh set holds: labels, orientations, vertex counts.
fn seen(frame: &XrFrame, attr: &str, mesh: bool, into: &mut Seen) {
    let set = get(frame, attr);
    if !present(&set) {
        return;
    }
    into.api = true;
    let Ok(Some(items)) = js_sys::try_iter(&set) else {
        return;
    };
    let (mut count, mut labels, mut orientations) = (0u32, BTreeMap::new(), BTreeMap::new());
    for item in items.flatten() {
        count += 1;
        let label = get_str(&item, "semanticLabel").unwrap_or_else(|| "unlabelled".into());
        *labels.entry(label).or_insert(0) += 1;
        if mesh {
            if let Some(n) = get_u32(&get(&item, "vertices"), "length") {
                into.max_vertices = into.max_vertices.max(n / 3);
            }
        } else {
            let o = get_str(&item, "orientation").unwrap_or_else(|| "unknown".into());
            *orientations.entry(o).or_insert(0) += 1;
        }
    }
    if count >= into.max_count {
        into.max_count = count;
        into.labels = labels;
        into.orientations = orientations;
    }
}

fn try_depth(p: &mut Probe, answer: Result<JsValue, String>) {
    p.tried = true;
    match answer {
        Ok(d) if present(&d) => {
            p.ok = true;
            p.error = None;
            p.width = get_u32(&d, "width");
            p.height = get_u32(&d, "height");
        }
        Ok(_) => p.error = Some("returned nothing yet".into()),
        Err(e) => p.error = Some(e),
    }
}

/// Open a camera in this 2D page, note its settings, and stop it at once. No frame is read.
/// `{ name: { exact: value } }`, a camera constraint that fails instead of settling for less.
fn exact(name: &str, value: &str) -> JsValue {
    let (outer, inner) = (Object::new(), Object::new());
    let _ = Reflect::set(&inner, &"exact".into(), &value.into());
    let _ = Reflect::set(&outer, &name.into(), &inner);
    outer.into()
}

fn check_camera(r: &Shared) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let devices = match w.navigator().media_devices() {
        Ok(d) => d,
        Err(e) => {
            r.borrow_mut().camera_2d = Some(Camera2d {
                error: Some(js_error(&e)),
                ..Default::default()
            });
            render(r);
            return;
        }
    };
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&JsValue::TRUE);
    let asked = devices.get_user_media_with_constraints(&constraints);
    set_text("status", "Asking for a headset camera…");
    let r = r.clone();
    spawn_local(async move {
        let mut camera = Camera2d::default();
        match asked {
            Err(e) => camera.error = Some(js_error(&e)),
            Ok(p) => match JsFuture::from(p).await {
                Err(e) => camera.error = Some(js_error(&e)),
                Ok(s) => {
                    let stream: MediaStream = s.unchecked_into();
                    if let Some(track) = stream.get_video_tracks().iter().next() {
                        if let Ok(settings) = get(&track, "getSettings")
                            .unchecked_into::<Function>()
                            .call0(&track)
                        {
                            camera.width = get_u32(&settings, "width");
                            camera.height = get_u32(&settings, "height");
                            camera.frame_rate = get(&settings, "frameRate").as_f64();
                        }
                    }
                    for t in stream.get_tracks().iter() {
                        t.unchecked_into::<MediaStreamTrack>().stop();
                    }
                    camera.opened = true;
                }
            },
        }
        // N8h.v3: every camera listed, each by itself, then the two facing modes by name, in
        // case the headset keeps a camera off the list
        let mut asks: Vec<(String, JsValue)> = vec![];
        if let Ok(p) = devices.enumerate_devices() {
            if let Ok(list) = JsFuture::from(p).await {
                let list = Array::from(&list);
                camera.devices = list
                    .iter()
                    .map(|d| Device {
                        kind: get_str(&d, "kind").unwrap_or_default(),
                        label: get_str(&d, "label").unwrap_or_default(),
                    })
                    .collect();
                for (i, d) in list
                    .iter()
                    .filter(|d| get_str(d, "kind").as_deref() == Some("videoinput"))
                    .enumerate()
                {
                    let Some(id) = get_str(&d, "deviceId").filter(|id| !id.is_empty()) else {
                        continue;
                    };
                    let label = get_str(&d, "label").filter(|l| !l.is_empty());
                    asks.push((
                        label.unwrap_or_else(|| format!("camera {i}")),
                        exact("deviceId", &id),
                    ));
                }
            }
        }
        asks.push((
            "facing environment".into(),
            exact("facingMode", "environment"),
        ));
        asks.push(("facing user".into(), exact("facingMode", "user")));
        if camera.opened {
            if let Some(t) = el("thumbs") {
                t.set_inner_html("");
            }
            for (i, (asked, video)) in asks.iter().enumerate() {
                set_text(
                    "status",
                    &format!(
                        "Looking through camera {} of {}: move your head slowly…",
                        i + 1,
                        asks.len()
                    ),
                );
                camera
                    .cameras
                    .push(try_camera(&devices, asked, video).await);
            }
        }
        set_text(
            "status",
            if camera.opened {
                "Every camera was opened, looked at and closed again. The small pictures below stay on this page."
            } else {
                "No camera opened in this page."
            },
        );
        r.borrow_mut().camera_2d = Some(camera);
        render(&r);
    });
}

fn copy(r: &Shared) {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(&r.borrow().json());
        set_text("status", "Copied the JSON.");
    }
}

/// Same pattern as room/src/journal.rs `download`.
fn download(r: &Shared) {
    let saved = (|| -> Result<(), String> {
        let parts = Array::new();
        parts.push(&r.borrow().json().into());
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("application/json");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)
            .map_err(|e| js_error(&e))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| js_error(&e))?;
        let a: web_sys::HtmlAnchorElement = doc()
            .create_element("a")
            .map_err(|e| js_error(&e))?
            .unchecked_into();
        a.set_href(&url);
        a.set_download("lucy-xr-check.json");
        doc()
            .body()
            .ok_or("no body")?
            .append_child(&a)
            .map_err(|e| js_error(&e))?;
        a.click();
        a.remove();
        let cleanup = Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        web_sys::window()
            .ok_or("no window")?
            .set_timeout_with_callback_and_timeout_and_arguments_0(cleanup.unchecked_ref(), 1000)
            .map_err(|e| js_error(&e))?;
        Ok(())
    })();
    set_text(
        "status",
        &match saved {
            Ok(()) => "Downloaded lucy-xr-check.json.".to_string(),
            Err(e) => format!("Download failed: {e}"),
        },
    );
}
