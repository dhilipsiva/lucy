//! Capturing a home and looking at it again (roadmap/N8j.md, N8j.v2.md), in the browser.
//!
//! `Capturing` runs inside a mixed-reality session the wearer started: it records the planes
//! and meshes the headset grants, the walked path, and every camera frame with the head pose
//! of each, as the camera gives them. The blur and the heavier processing happen on the
//! desktop. A raw capture opened here in a browser is blurred by my face finder's worker
//! before it is stored. `Viewing` draws a stored capture, its pictures laid over its surfaces.

use super::browser::{call, get, pose_matrix, present};
use super::project;
use crate::colors::linear;
use crate::gl::{Gl, GpuMesh, Lights, LitParams, Texture};
use crate::sources::js_error;
use glam::{Mat4, Quat, Vec3};
use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use lucy_room_memory::blur::{self, Detection, FaceBox};
use lucy_room_memory::home::{
    base64_decode, base64_encode, Alignment, Detector, HomeCapture, PathPoint, Photo, Plane, Stats,
    SurfaceMesh, FORMAT, MAX_PATH, MAX_PHOTOS,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Blob, CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, ImageData, MediaStream,
    MediaStreamConstraints, MediaStreamTrack, Worker, WorkerOptions, WorkerType, XrFrame,
    XrReferenceSpace,
};

/// How long the face finder may take over one picture before the file is refused.
const DETECT_MS: i32 = 10_000;
/// The path is kept at thirty points a second.
const PATH_STEP_S: f32 = 1.0 / 30.0;

fn doc() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}
fn canvas(w: u32, h: u32) -> Option<(HtmlCanvasElement, CanvasRenderingContext2d)> {
    let c: HtmlCanvasElement = doc().create_element("canvas").ok()?.unchecked_into();
    c.set_width(w);
    c.set_height(h);
    let ctx = c.get_context("2d").ok()??.dyn_into().ok()?;
    Some((c, ctx))
}
fn iso() -> String {
    String::from(js_sys::Date::new_0().to_iso_string())
}

// ── capturing ──

#[derive(Default)]
struct Cam {
    stream: Option<MediaStream>,
    video: Option<JsValue>,
    error: Option<String>,
}

/// A capture in progress (roadmap/N8j.v2.md): everything is kept as the headset gives it.
/// Every camera frame is stored as taken, faces and all; the blur happens on the desktop,
/// before my store will take the file.
pub struct Capturing {
    cam: Rc<RefCell<Cam>>,
    frame: Option<(HtmlCanvasElement, CanvasRenderingContext2d)>,
    last_total: Option<u32>,
    /// Why pictures were lost, if any were: counted so the file can say.
    failures: BTreeMap<&'static str, u32>,
    next_id: u32,
    done: Vec<(Photo, Blob)>,
    planes: Vec<Plane>,
    meshes: Vec<SurfaceMesh>,
    path: Vec<PathPoint>,
    offered: u32,
    started_ms: Option<f64>,
    started: String,
    frames: u64,
    floor: bool,
}

impl Capturing {
    /// Runs inside the visitor's press: the camera is asked for now and not waited for; the
    /// capture copes with it being absent.
    pub fn begin() -> Capturing {
        let cam = Rc::new(RefCell::new(Cam::default()));
        open_camera(&cam);
        Capturing {
            cam,
            frame: None,
            last_total: None,
            failures: BTreeMap::new(),
            next_id: 0,
            done: vec![],
            planes: vec![],
            meshes: vec![],
            path: vec![],
            offered: 0,
            started_ms: None,
            started: iso(),
            frames: 0,
            floor: true,
        }
    }

    /// One headset frame. Returns the lines for the board in front of the wearer.
    pub fn frame(
        &mut self,
        now: f64,
        frame: &XrFrame,
        space: &XrReferenceSpace,
        floor: bool,
        head: Mat4,
    ) -> Vec<(f64, String)> {
        self.frames += 1;
        self.floor = floor;
        let start = *self.started_ms.get_or_insert(now);
        let t = ((now - start) / 1000.0) as f32;
        if self.path.len() < MAX_PATH && self.path.last().is_none_or(|p| t - p.t >= PATH_STEP_S) {
            let (_, rot, pos) = head.to_scale_rotation_translation();
            self.path.push(PathPoint {
                t,
                pos: pos.to_array(),
                quat: rot.to_array(),
            });
        }
        if self.frames % 30 == 1 {
            self.read_surfaces(frame, space);
        }
        self.take(t, head);
        self.lines()
    }

    fn read_surfaces(&mut self, frame: &XrFrame, space: &XrReferenceSpace) {
        let label = |v: &JsValue| {
            get(v, "semanticLabel")
                .as_string()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unlabelled".into())
        };
        let pose_of = |v: &JsValue, key: &str| {
            call(frame, "getPose", &Array::of2(&get(v, key), space))
                .ok()
                .and_then(|p| pose_matrix(&p))
        };
        let set = get(frame, "detectedPlanes");
        if present(&set) {
            if let Ok(Some(items)) = js_sys::try_iter(&set) {
                let mut planes = vec![];
                for plane in items.flatten() {
                    let Some(pose) = pose_of(&plane, "planeSpace") else {
                        continue;
                    };
                    let polygon: Vec<[f32; 3]> = Array::from(&get(&plane, "polygon"))
                        .iter()
                        .map(|p| {
                            let f = |k| get(&p, k).as_f64().unwrap_or(0.0) as f32;
                            [f("x"), f("y"), f("z")]
                        })
                        .collect();
                    planes.push(Plane {
                        label: label(&plane),
                        orientation: get(&plane, "orientation")
                            .as_string()
                            .unwrap_or_else(|| "unknown".into()),
                        pose: pose.to_cols_array(),
                        polygon,
                    });
                }
                if !planes.is_empty() {
                    self.planes = planes;
                }
            }
        }
        let set = get(frame, "detectedMeshes");
        if present(&set) {
            if let Ok(Some(items)) = js_sys::try_iter(&set) {
                let mut meshes = vec![];
                for mesh in items.flatten() {
                    let Some(pose) = pose_of(&mesh, "meshSpace") else {
                        continue;
                    };
                    let vertices = js_sys::Float32Array::new(&get(&mesh, "vertices")).to_vec();
                    let indices = js_sys::Uint32Array::new(&get(&mesh, "indices")).to_vec();
                    meshes.push(SurfaceMesh {
                        label: label(&mesh),
                        pose: pose.to_cols_array(),
                        vertices,
                        indices,
                    });
                }
                if !meshes.is_empty() {
                    self.meshes = meshes;
                }
            }
        }
    }

    /// Keep the camera's frame if it delivered a new one: as taken, at its own size.
    fn take(&mut self, t: f32, head: Mat4) {
        let Some(video) = self.cam.borrow().video.clone() else {
            return;
        };
        let total = call(&video, "getVideoPlaybackQuality", &Array::new())
            .ok()
            .and_then(|q| get(&q, "totalVideoFrames").as_f64())
            .map(|v| v as u32);
        let Some(total) = total else { return };
        let fresh = self.last_total != Some(total);
        self.last_total = Some(total);
        if !fresh || get(&video, "readyState").as_f64().unwrap_or(0.0) < 2.0 {
            return;
        }
        self.offered += 1;
        if self.next_id as usize >= MAX_PHOTOS {
            return self.lost("the cap on pictures was reached");
        }
        let w = get(&video, "videoWidth").as_f64().unwrap_or(640.0).max(1.0) as u32;
        let h = get(&video, "videoHeight")
            .as_f64()
            .unwrap_or(480.0)
            .max(1.0) as u32;
        if self
            .frame
            .as_ref()
            .is_none_or(|(c, _)| c.width() != w || c.height() != h)
        {
            self.frame = canvas(w, h);
        }
        let Some((canvas, ctx)) = &self.frame else {
            return self.lost("no canvas");
        };
        let Ok(draw) = get(ctx, "drawImage").dyn_into::<Function>() else {
            return self.lost("no drawImage");
        };
        let args = Array::of5(&video, &0.into(), &0.into(), &w.into(), &h.into());
        if Reflect::apply(&draw, ctx, &args).is_err() {
            return self.lost("the frame could not be drawn");
        }
        // Encoded here and now. canvas.toBlob answers from the browser's idle time, and a page
        // behind an immersive session may get none: on his headset the first build kept no
        // picture at all.
        let jpeg = canvas
            .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.9))
            .ok()
            .and_then(|url| {
                url.strip_prefix("data:image/jpeg;base64,")
                    .and_then(|b64| base64_decode(b64).ok())
            })
            .filter(|bytes| bytes.len() > 3 && bytes[..3] == [0xff, 0xd8, 0xff]);
        let Some(blob) = jpeg.and_then(|bytes| jpeg_blob(&bytes).ok()) else {
            return self.lost("the picture did not encode");
        };
        let photo = Photo {
            id: self.next_id,
            t,
            pose: head.to_cols_array(),
            width: w,
            height: h,
            faces_blurred: 0,
            jpeg_base64: String::new(),
        };
        self.next_id += 1;
        self.done.push((photo, blob));
    }

    fn lost(&mut self, why: &'static str) {
        *self.failures.entry(why).or_insert(0) += 1;
    }

    fn kept(&self) -> u32 {
        self.done.len() as u32
    }

    fn lines(&self) -> Vec<(f64, String)> {
        let cam = self.cam.borrow();
        let camera = if let Some(e) = &cam.error {
            format!("no camera: {e}")
        } else if cam.video.is_none() {
            "camera starting…".into()
        } else {
            format!(
                "● camera on · {} pictures kept of {} frames",
                self.kept(),
                self.offered
            )
        };
        vec![
            (50.0, camera),
            (
                44.0,
                format!(
                    "{} surfaces · {} meshes · walk slowly · B: finish",
                    self.planes.len(),
                    self.meshes.len()
                ),
            ),
            (34.0, "afterwards: Garage → Download my capture".into()),
        ]
    }

    /// What a test or a screen reader can read of the capture as it runs.
    pub fn status(&self) -> String {
        format!(
            "garage capture · planes {} · meshes {} · path {} · offered {} · kept {} · raw",
            self.planes.len(),
            self.meshes.len(),
            self.path.len(),
            self.offered,
            self.kept()
        )
    }

    /// The capture ends: camera off, and what was gathered handed over, marked raw.
    pub fn finish(mut self) -> (HomeCapture, Vec<(u32, Blob)>) {
        close_camera(&self.cam);
        let mut kept: Vec<(Photo, Blob)> = std::mem::take(&mut self.done);
        kept.sort_by_key(|(p, _)| p.id);
        let mut notes = vec![
            "Raw: the pictures are as the camera gave them; faces are not blurred yet. Run lucy-home-blur on the desktop before lucy home import.".to_string(),
            "Depth is not captured: the headset gives it to the graphics card only.".into(),
            "The camera's place and lens are a starting guess; align them in the viewer.".into(),
            "Frames come from a video element, about one a second on a Quest 3.".into(),
        ];
        if let Some(e) = &self.cam.borrow().error {
            notes.push(format!("No camera: {e}"));
        } else if self.cam.borrow().video.is_none() {
            notes.push("The camera never started during this capture.".into());
        } else if self.offered == 0 {
            notes.push("The camera started but delivered no frame during this capture.".into());
        }
        for (why, n) in &self.failures {
            notes.push(format!("{n} frames were lost: {why}."));
        }
        let stamp: String = self
            .started
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        let capture = HomeCapture {
            format: FORMAT.into(),
            id: format!("home-{stamp}"),
            started: self.started.clone(),
            ended: iso(),
            device: web_sys::window()
                .and_then(|w| w.navigator().user_agent().ok())
                .unwrap_or_default(),
            reference_space: if self.floor { "local-floor" } else { "local" }.into(),
            raw: true,
            planes: std::mem::take(&mut self.planes),
            meshes: std::mem::take(&mut self.meshes),
            path: std::mem::take(&mut self.path),
            photos: kept.iter().map(|(p, _)| p.clone()).collect(),
            stats: Stats {
                offered: self.offered,
                kept: kept.len() as u32,
                dropped: self.offered.saturating_sub(kept.len() as u32),
                faces_blurred: 0,
            },
            detector: Detector::default(),
            alignment: Alignment::default(),
            notes,
        };
        (capture, kept.into_iter().map(|(p, b)| (p.id, b)).collect())
    }
}

fn open_camera(cam: &Rc<RefCell<Cam>>) {
    let fail = |e: String| cam.borrow_mut().error = Some(e);
    let Some(w) = web_sys::window() else { return };
    let devices = match w.navigator().media_devices() {
        Ok(d) => d,
        Err(e) => return fail(js_error(&e)),
    };
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&JsValue::TRUE);
    let asked = match devices.get_user_media_with_constraints(&constraints) {
        Ok(p) => p,
        Err(e) => return fail(js_error(&e)),
    };
    let cam = cam.clone();
    spawn_local(async move {
        match JsFuture::from(asked).await {
            Err(e) => cam.borrow_mut().error = Some(js_error(&e)),
            Ok(s) => {
                let stream: MediaStream = s.unchecked_into();
                let Ok(video) = doc().create_element("video") else {
                    cam.borrow_mut().error = Some("no video element".into());
                    return;
                };
                let video: JsValue = video.into();
                let _ = Reflect::set(&video, &"muted".into(), &JsValue::TRUE);
                let _ = Reflect::set(&video, &"playsInline".into(), &JsValue::TRUE);
                let _ = Reflect::set(&video, &"srcObject".into(), &stream);
                let _ = call(&video, "play", &Array::new());
                let mut c = cam.borrow_mut();
                c.stream = Some(stream);
                c.video = Some(video);
            }
        }
    });
}

fn close_camera(cam: &Rc<RefCell<Cam>>) {
    let mut c = cam.borrow_mut();
    if let Some(stream) = c.stream.take() {
        for t in stream.get_tracks().iter() {
            t.unchecked_into::<MediaStreamTrack>().stop();
        }
    }
    if let Some(v) = c.video.take() {
        let _ = Reflect::set(&v, &"srcObject".into(), &JsValue::NULL);
    }
}

// ── the face finder, for a raw capture opened in a browser ──

/// A promise and the function that settles it.
fn deferred() -> (js_sys::Promise, Function) {
    let mut resolve = None;
    let promise = js_sys::Promise::new(&mut |ok, _| resolve = Some(ok));
    (promise, resolve.expect("the executor runs at once"))
}
/// Wait for a promise, but only so long; None when the time ran out first.
async fn within(promise: &js_sys::Promise, ms: i32) -> Option<JsValue> {
    let (timeout, fire) = deferred();
    let window = web_sys::window()?;
    let late = Closure::once_into_js(move || {
        let _ = fire.call1(&JsValue::NULL, &JsValue::NULL);
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(late.unchecked_ref(), ms);
    let first = JsFuture::from(js_sys::Promise::race(&Array::of2(promise, &timeout)))
        .await
        .ok()?;
    (!first.is_null()).then_some(first)
}

#[derive(Default)]
struct FinderState {
    hello: Option<Function>,
    waiting: BTreeMap<u32, Function>,
}

/// My face finder's worker, asked one picture at a time.
struct Finder {
    worker: Worker,
    state: Rc<RefCell<FinderState>>,
    name: String,
    sha: String,
    next: u32,
}
impl Finder {
    async fn start() -> Result<Finder, String> {
        let url =
            crate::sources::worker_url("face")?.replace("worker.js?role=face", "face-worker.js");
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(&url, &options).map_err(|e| js_error(&e))?;
        let state = Rc::new(RefCell::new(FinderState::default()));
        let (hello, said) = deferred();
        state.borrow_mut().hello = Some(said);
        let s = state.clone();
        let on =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                let m = e.data();
                let mut st = s.borrow_mut();
                let settle = match get(&m, "id").as_f64() {
                    Some(id) => st.waiting.remove(&(id as u32)),
                    None => st.hello.take(),
                };
                drop(st);
                if let Some(f) = settle {
                    let _ = f.call1(&JsValue::NULL, &m);
                }
            });
        worker.set_onmessage(Some(on.as_ref().unchecked_ref()));
        on.forget();
        let s = state.clone();
        let fail = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if let Some(f) = s.borrow_mut().hello.take() {
                let gone = Object::new();
                let _ = Reflect::set(&gone, &"fatal".into(), &"it is not on this site".into());
                let _ = f.call1(&JsValue::NULL, &gone);
            }
        });
        worker.set_onerror(Some(fail.as_ref().unchecked_ref()));
        fail.forget();
        let first = within(&hello, 30_000).await;
        let ready = first.as_ref().is_some_and(|m| get(m, "ready").is_truthy());
        if !ready {
            worker.terminate();
            return Err(first
                .and_then(|m| get(&m, "fatal").as_string())
                .unwrap_or_else(|| "it did not start in time".into()));
        }
        let m = first.unwrap_or(JsValue::UNDEFINED);
        Ok(Finder {
            worker,
            state,
            name: get(&m, "name").as_string().unwrap_or_default(),
            sha: get(&m, "model_sha256").as_string().unwrap_or_default(),
            next: 0,
        })
    }

    async fn detect(&mut self, rgba: &[u8], w: u32, h: u32) -> Detection {
        let id = self.next;
        self.next += 1;
        let (answer, settle) = deferred();
        self.state.borrow_mut().waiting.insert(id, settle);
        let msg = Object::new();
        let _ = Reflect::set(&msg, &"id".into(), &id.into());
        let _ = Reflect::set(&msg, &"width".into(), &w.into());
        let _ = Reflect::set(&msg, &"height".into(), &h.into());
        let _ = Reflect::set(&msg, &"rgba".into(), &Uint8Array::from(rgba));
        if let Err(e) = self.worker.post_message(&msg) {
            return Detection::Failed(js_error(&e));
        }
        let Some(m) = within(&answer, DETECT_MS).await else {
            self.state.borrow_mut().waiting.remove(&id);
            return Detection::TimedOut;
        };
        if !get(&m, "ok").is_truthy() {
            return Detection::Failed(
                get(&m, "error")
                    .as_string()
                    .unwrap_or_else(|| "no reason given".into()),
            );
        }
        Detection::Found(
            js_sys::Float32Array::new(&get(&m, "boxes"))
                .to_vec()
                .chunks(5)
                .filter(|b| b.len() == 5)
                .map(|b| FaceBox {
                    x1: b[0],
                    y1: b[1],
                    x2: b[2],
                    y2: b[3],
                })
                .collect(),
        )
    }
}

/// One raw picture, blurred: decoded, read by the finder, its faces pixelated, encoded again.
async fn blur_one(finder: &mut Finder, jpeg: &[u8]) -> Result<(Blob, u32, u32, u32), String> {
    let window = web_sys::window().ok_or("no window")?;
    let bitmap: ImageBitmap = JsFuture::from(js_sys::Promise::from(call(
        &window,
        "createImageBitmap",
        &Array::of1(&jpeg_blob(jpeg)?.into()),
    )?))
    .await
    .map_err(|e| js_error(&e))?
    .unchecked_into();
    let (w, h) = (bitmap.width(), bitmap.height());
    let (c, ctx) = canvas(w, h).ok_or("no canvas")?;
    ctx.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
        .map_err(|e| js_error(&e))?;
    bitmap.close();
    let mut rgba = ctx
        .get_image_data(0, 0, w as i32, h as i32)
        .map_err(|e| js_error(&e))?
        .data()
        .0;
    let detection = finder.detect(&rgba, w, h).await;
    let faces = blur::process(&mut rgba, w as usize, h as usize, &detection)?;
    let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), w, h)
        .map_err(|e| js_error(&e))?;
    ctx.put_image_data(&image, 0, 0).map_err(|e| js_error(&e))?;
    let (encoded, settle) = deferred();
    let cb = Closure::once_into_js(move |blob: JsValue| {
        let _ = settle.call1(&JsValue::NULL, &blob);
    });
    c.to_blob_with_type_and_encoder_options(
        cb.unchecked_ref(),
        "image/jpeg",
        &JsValue::from_f64(0.9),
    )
    .map_err(|e| js_error(&e))?;
    let blob = JsFuture::from(encoded)
        .await
        .ok()
        .and_then(|b| b.dyn_into::<Blob>().ok())
        .ok_or("the blurred picture did not encode")?;
    Ok((blob, faces, w, h))
}

// ── storing, exporting, importing ──

fn jpeg_blob(bytes: &[u8]) -> Result<Blob, String> {
    let parts = Array::new();
    parts.push(&Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("image/jpeg");
    Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(|e| js_error(&e))
}

async fn bytes_of(blob: &Blob) -> Result<Vec<u8>, String> {
    let buffer = JsFuture::from(blob.array_buffer())
        .await
        .map_err(|e| js_error(&e))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Keep a finished capture in this browser, replacing the one before.
pub async fn store(capture: &HomeCapture, photos: &[(u32, Blob)]) -> Result<(), String> {
    crate::journal::capture_put(&capture.without_pictures().json()?, photos).await
}

/// Tell the room what capture this browser keeps, so the panels can say where its file is.
pub fn refresh(app: std::rc::Weak<std::cell::RefCell<crate::app::App>>) {
    spawn_local(async move {
        let line = crate::journal::capture_meta()
            .await
            .ok()
            .flatten()
            .and_then(|meta| HomeCapture::parse(&meta).ok())
            .map(|c| format!("{} surfaces, {} pictures", c.planes.len(), c.photos.len()));
        if let Some(a) = app.upgrade() {
            a.borrow_mut().garage_capture(line);
        }
    });
}

async fn stored() -> Result<Option<(HomeCapture, Vec<(u32, Blob)>)>, String> {
    let Some(meta) = crate::journal::capture_meta().await? else {
        return Ok(None);
    };
    let capture = HomeCapture::parse(&meta)?;
    let ids: Vec<u32> = capture.photos.iter().map(|p| p.id).collect();
    let photos = crate::journal::capture_photos(&ids).await?;
    Ok(Some((capture, photos)))
}

/// The stored capture as one JSON file, pictures inside, handed to the visitor. The file is
/// built in pieces, one per picture, so a long walk never becomes one giant string.
pub async fn download() -> Result<String, String> {
    let (mut capture, photos) = stored()
        .await?
        .ok_or("There's no captured home in this browser yet.")?;
    let have: BTreeMap<u32, &Blob> = photos.iter().map(|(id, b)| (*id, b)).collect();
    // a picture whose bytes are gone is left out, never exported empty
    capture.photos.retain(|p| have.contains_key(&p.id));
    let json = capture.without_pictures().json()?;
    // every picture's field is empty in that text, in order: the bytes go in between
    const EMPTY: &str = "\"jpeg_base64\":\"\"";
    let pieces: Vec<&str> = json.split(EMPTY).collect();
    if pieces.len() != capture.photos.len() + 1 {
        return Err("the capture's pictures and its text disagree".into());
    }
    let parts = Array::new();
    let mut bytes = 0usize;
    let mut push = |s: &str| {
        bytes += s.len();
        parts.push(&JsValue::from_str(s));
    };
    for (i, piece) in pieces.iter().enumerate() {
        push(piece);
        if let Some(p) = capture.photos.get(i) {
            push("\"jpeg_base64\":\"");
            push(&base64_encode(&bytes_of(have[&p.id]).await?));
            push("\"");
        }
    }
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob =
        Blob::new_with_str_sequence_and_options(&parts, &options).map_err(|e| js_error(&e))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| js_error(&e))?;
    let a: web_sys::HtmlAnchorElement = doc()
        .create_element("a")
        .map_err(|e| js_error(&e))?
        .unchecked_into();
    a.set_href(&url);
    a.set_download("lucy-home-capture.json");
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
    let _ = web_sys::window()
        .ok_or("no window")?
        .set_timeout_with_callback_and_timeout_and_arguments_0(cleanup.unchecked_ref(), 30_000);
    Ok(format!(
        "Downloaded lucy-home-capture.json: {} surfaces, {} pictures, {:.1} MB.{}",
        capture.planes.len(),
        capture.photos.len(),
        bytes as f64 / 1_048_576.0,
        if capture.raw {
            " The pictures are as taken: run lucy-home-blur on your desktop before lucy home import."
        } else {
            ""
        }
    ))
}

/// A capture file the visitor picked: checked strictly, then kept in this browser. A raw one
/// is blurred first, picture by picture; if my face finder isn't here, it is refused.
pub async fn import(text: &str) -> Result<String, String> {
    let mut capture = HomeCapture::parse(text)?;
    let mut finder = if capture.raw {
        Some(Finder::start().await.map_err(|e| {
            format!("this is a raw capture, and I can't blur its faces here ({e}). Run lucy-home-blur on the desktop and open the file it writes")
        })?)
    } else {
        None
    };
    let mut photos = vec![];
    let mut faces = 0;
    let result: Result<(), String> = async {
        for p in &mut capture.photos {
            if p.jpeg_base64.is_empty() {
                continue;
            }
            let bytes = base64_decode(&p.jpeg_base64)?;
            if bytes.len() < 3 || bytes[..3] != [0xff, 0xd8, 0xff] {
                return Err(format!("picture {} is not a JPEG", p.id));
            }
            let blob = match finder.as_mut() {
                Some(f) => {
                    let (blob, n, w, h) = blur_one(f, &bytes)
                        .await
                        .map_err(|e| format!("picture {}: {e}", p.id))?;
                    p.faces_blurred = n;
                    p.width = w;
                    p.height = h;
                    faces += n;
                    blob
                }
                None => jpeg_blob(&bytes)?,
            };
            photos.push((p.id, blob));
        }
        Ok(())
    }
    .await;
    if let Some(f) = finder.take() {
        f.worker.terminate();
        if result.is_ok() {
            capture.raw = false;
            capture.stats.faces_blurred = faces;
            capture.detector = Detector {
                name: f.name,
                model_sha256: f.sha,
            };
            capture
                .notes
                .push("Faces were blurred in the browser when this raw capture was opened.".into());
        }
    }
    result?;
    store(&capture, &photos).await?;
    Ok(format!(
        "Opened a captured home: {} surfaces, {} meshes, {} pictures.",
        capture.planes.len(),
        capture.meshes.len(),
        photos.len()
    ))
}

/// Let the visitor pick a capture file. Runs inside their click; `then` gets what happened.
pub fn pick(then: impl Fn(Result<String, String>) + 'static) {
    let Ok(el) = doc().create_element("input") else {
        return;
    };
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    input.set_type("file");
    input.set_accept("application/json,.json");
    let picked = input.clone();
    let then = Rc::new(then);
    let cb = Closure::<dyn FnMut()>::new(move || {
        let Some(file) = picked.files().and_then(|f| f.get(0)) else {
            return;
        };
        let then = then.clone();
        spawn_local(async move {
            let result = match JsFuture::from(file.text()).await {
                Ok(t) => import(&t.as_string().unwrap_or_default()).await,
                Err(e) => Err(js_error(&e)),
            };
            then(result);
        });
    });
    input.set_onchange(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
    input.click();
}

// ── viewing ──

/// A stored capture read for viewing: its pictures decoded, small when there are many.
pub struct Loaded {
    capture: HomeCapture,
    bitmaps: BTreeMap<u32, ImageBitmap>,
}

thread_local! {
    static ARRIVED: RefCell<Option<Result<Loaded, String>>> = const { RefCell::new(None) };
}

/// Read the stored capture in the background; `Viewing` picks it up when it lands.
pub fn load() {
    ARRIVED.with(|a| *a.borrow_mut() = None);
    spawn_local(async {
        let result = async {
            let (capture, photos) = stored()
                .await?
                .ok_or("There's no captured home in this browser yet. Capture one on a headset, or open a capture file.")?;
            let small = photos.len() > 100;
            let window = web_sys::window().ok_or("no window")?;
            let mut bitmaps = BTreeMap::new();
            for (id, blob) in photos {
                let args = Array::of1(&blob);
                if small {
                    let options = Object::new();
                    let _ = Reflect::set(&options, &"resizeWidth".into(), &320.into());
                    let _ = Reflect::set(&options, &"resizeHeight".into(), &240.into());
                    args.push(&options);
                }
                let promise = call(&window, "createImageBitmap", &args)?;
                if let Ok(b) = JsFuture::from(js_sys::Promise::from(promise)).await {
                    bitmaps.insert(id, b.unchecked_into());
                }
            }
            Ok::<_, String>(Loaded { capture, bitmaps })
        }
        .await;
        ARRIVED.with(|a| *a.borrow_mut() = Some(result));
    });
}

#[derive(Default)]
struct PanelState {
    alignment: Option<Alignment>,
    save: bool,
    leave: bool,
}

/// What moves the viewer this frame.
#[derive(Clone, Copy, Default)]
pub struct Walk {
    pub forward: f32,
    pub strafe: f32,
    pub turn: f32,
    pub look: f32,
    pub rise: f32,
}

pub struct Viewing {
    capture: Option<HomeCapture>,
    bitmaps: BTreeMap<u32, ImageBitmap>,
    textures: BTreeMap<usize, Texture>,
    pictures: Vec<(usize, GpuMesh)>,
    bare: Option<GpuMesh>,
    alignment: Alignment,
    dirty: bool,
    built_at: f64,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    panel: Option<web_sys::Element>,
    shared: Rc<RefCell<PanelState>>,
    pub error: Option<String>,
    pub leave: bool,
    summary: String,
    lights: Lights,
}

impl Viewing {
    pub fn new() -> Viewing {
        Viewing {
            capture: None,
            bitmaps: BTreeMap::new(),
            textures: BTreeMap::new(),
            pictures: vec![],
            bare: None,
            alignment: Alignment::default(),
            dirty: false,
            built_at: 0.0,
            pos: Vec3::new(0.0, 1.6, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            panel: None,
            shared: Rc::new(RefCell::new(PanelState::default())),
            error: None,
            leave: false,
            summary: "loading".into(),
            lights: Lights {
                hemi_sky: linear(0xE6EAF0),
                hemi_ground: linear(0x6A655E),
                hemi_intensity: 1.2,
                dir_color: linear(0xFFF4E6),
                dir_dir: Vec3::new(0.3, 1.0, 0.5).normalize(),
                dir_intensity: 0.8,
                points: vec![],
                fog_color: [0.0; 3],
                fog_near: 1.0e6,
                fog_far: 2.0e6,
            },
        }
    }

    /// One frame of the viewer's own life: take the capture when it lands, hear the panel,
    /// re-paint when the alignment changed, and walk.
    pub fn step(&mut self, gl: &Gl, now: f64, dt: f32, walk: Walk, flat: bool) {
        if self.capture.is_none() && self.error.is_none() {
            match ARRIVED.with(|a| a.borrow_mut().take()) {
                Some(Ok(loaded)) => {
                    self.alignment = loaded.capture.alignment;
                    if let Some(p) = loaded.capture.path.first() {
                        self.pos = Vec3::from(p.pos);
                        let f = Quat::from_array(p.quat) * -Vec3::Z;
                        self.yaw = (-f.x).atan2(-f.z);
                    }
                    self.bitmaps = loaded.bitmaps;
                    self.capture = Some(loaded.capture);
                    self.dirty = true;
                    if flat {
                        self.build_panel();
                    }
                }
                Some(Err(e)) => self.error = Some(e),
                None => {}
            }
        }
        {
            let mut s = self.shared.borrow_mut();
            if let Some(a) = s.alignment.take() {
                self.alignment = a;
                self.dirty = true;
            }
            if std::mem::take(&mut s.leave) {
                self.leave = true;
            }
            if std::mem::take(&mut s.save) {
                if let Some(c) = &mut self.capture {
                    c.alignment = self.alignment;
                    if let Ok(json) = c.without_pictures().json() {
                        spawn_local(async move {
                            let _ = crate::journal::capture_meta_put(&json).await;
                        });
                    }
                }
            }
        }
        if self.dirty && now - self.built_at > 250.0 {
            self.paint(gl);
            self.built_at = now;
            self.dirty = false;
        }
        self.yaw += walk.turn * dt;
        self.pitch = (self.pitch + walk.look * dt).clamp(-1.4, 1.4);
        let facing = Quat::from_rotation_y(self.yaw);
        self.pos += facing * Vec3::new(walk.strafe, 0.0, -walk.forward) * 1.6 * dt;
        self.pos.y += walk.rise * dt;
    }

    fn paint(&mut self, gl: &Gl) {
        let Some(capture) = &self.capture else { return };
        let painted = project::paint(capture, &self.alignment);
        if let Some(old) = self.bare.take() {
            gl.delete_mesh(old);
        }
        for (_, old) in self.pictures.drain(..) {
            gl.delete_mesh(old);
        }
        if !painted.bare.indices.is_empty() {
            self.bare = Some(gl.upload(&painted.bare));
        }
        for (index, mesh) in &painted.pictures {
            if !self.textures.contains_key(index) {
                let id = capture.photos[*index].id;
                let Some(bitmap) = self.bitmaps.remove(&id) else {
                    continue;
                };
                let Some((c, ctx)) = canvas(bitmap.width(), bitmap.height()) else {
                    continue;
                };
                if ctx.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0).is_ok() {
                    self.textures
                        .insert(*index, gl.texture_from_canvas(&c, false));
                }
                bitmap.close();
            }
            if self.textures.contains_key(index) {
                self.pictures.push((*index, gl.upload(mesh)));
            }
        }
        self.summary = format!(
            "garage home · planes {} · meshes {} · pictures {} · cells {} · textured {} · used {}",
            capture.planes.len(),
            capture.meshes.len(),
            capture.photos.len(),
            painted.cells,
            painted.textured,
            self.pictures.len()
        );
        if let Some(e) = self
            .panel
            .as_ref()
            .and_then(|p| p.query_selector("[data-role=stats]").ok().flatten())
        {
            e.set_text_content(Some(&format!(
                "{} of {} pieces of surface carry a picture, from {} of {} pictures.",
                painted.textured,
                painted.cells,
                self.pictures.len(),
                capture.photos.len()
            )));
        }
    }

    pub fn status(&self) -> String {
        match &self.error {
            Some(e) => format!("garage home · error: {e}"),
            None => self.summary.clone(),
        }
    }
    pub fn ready(&self) -> bool {
        self.capture.is_some()
    }

    /// Draw the home from one eye.
    pub fn render(&self, gl: &Gl, view: &Mat4, proj: &Mat4, cam: Vec3) {
        if let Some(bare) = &self.bare {
            gl.draw_lit(
                bare,
                &LitParams {
                    model: Mat4::IDENTITY,
                    color: linear(0x9AA3AD),
                    emissive: [0.0; 3],
                    emissive_intensity: 0.0,
                    roughness: 0.9,
                    opacity: 1.0,
                    map: None,
                    double_sided: true,
                    depth_write: true,
                },
                view,
                proj,
                &self.lights,
                cam,
            );
        }
        let mvp = *proj * *view;
        for (index, mesh) in &self.pictures {
            if let Some(t) = self.textures.get(index) {
                gl.draw_photo(mesh, &mvp, t);
            }
        }
    }

    /// The alignment panel: where the camera sits against the head and how wide it sees.
    fn build_panel(&mut self) {
        let d = doc();
        let Ok(panel) = d.create_element("div") else {
            return;
        };
        let _ = panel.set_attribute("id", "home-panel");
        let _ = panel.set_attribute(
            "style",
            "position:fixed;top:12px;right:12px;z-index:30;width:min(320px,90vw);max-height:90vh;overflow:auto;padding:12px 14px;border-radius:10px;background:rgba(12,14,18,.88);color:#F4F3EF;font:13px/1.4 'IBM Plex Sans',system-ui,sans-serif",
        );
        let add = |tag: &str, text: &str, style: &str| -> Option<web_sys::Element> {
            let e = d.create_element(tag).ok()?;
            e.set_text_content(Some(text));
            if !style.is_empty() {
                let _ = e.set_attribute("style", style);
            }
            panel.append_child(&e).ok()?;
            Some(e)
        };
        add(
            "strong",
            "Line the pictures up",
            "display:block;margin-bottom:4px",
        );
        add(
            "p",
            "I don't know exactly where the headset's camera sits or how wide it sees. Move these until the pictures sit on the walls. W A S D walks, drag looks, R and F rise and sink.",
            "margin:0 0 8px;color:#B9B6AE",
        );
        let a = self.alignment;
        let fields: [(&str, &str, f32, f32, f32, f32); 8] = [
            ("hfov_deg", "Lens width (°)", 40.0, 130.0, 0.5, a.hfov_deg),
            (
                "yaw_deg",
                "Turn left / right (°)",
                -30.0,
                30.0,
                0.25,
                a.yaw_deg,
            ),
            (
                "pitch_deg",
                "Tilt up / down (°)",
                -30.0,
                30.0,
                0.25,
                a.pitch_deg,
            ),
            ("roll_deg", "Roll (°)", -20.0, 20.0, 0.25, a.roll_deg),
            ("dx", "Right of the eyes (m)", -0.15, 0.15, 0.002, a.dx),
            ("dy", "Above the eyes (m)", -0.15, 0.15, 0.002, a.dy),
            ("dz", "Behind the eyes (m)", -0.2, 0.1, 0.002, a.dz),
            (
                "lag_s",
                "Picture lags the pose (s)",
                -0.5,
                1.5,
                0.01,
                a.lag_s,
            ),
        ];
        for (key, label, lo, hi, step, value) in fields {
            let Ok(row) = d.create_element("label") else {
                continue;
            };
            let _ = row.set_attribute("style", "display:block;margin:6px 0");
            row.set_text_content(Some(label));
            let Ok(input) = d.create_element("input") else {
                continue;
            };
            let input: web_sys::HtmlInputElement = input.unchecked_into();
            input.set_type("range");
            let _ = input.set_attribute("min", &lo.to_string());
            let _ = input.set_attribute("max", &hi.to_string());
            let _ = input.set_attribute("step", &step.to_string());
            let _ = input.set_attribute("data-field", key);
            let _ = input.set_attribute("id", &format!("home-{key}"));
            let _ = input.set_attribute("style", "display:block;width:100%");
            input.set_value(&value.to_string());
            let _ = row.append_child(&input);
            let _ = panel.append_child(&row);
        }
        add("p", "", "margin:8px 0;color:#B9B6AE").map(|e| e.set_attribute("data-role", "stats"));
        for (role, text) in [("save", "Keep this alignment"), ("leave", "Leave")] {
            if let Some(b) = add(
                "button",
                text,
                "margin:4px 6px 0 0;padding:6px 10px;border-radius:6px;border:1px solid #555;background:#1a1d22;color:#F4F3EF;font:inherit;cursor:pointer",
            ) {
                let _ = b.set_attribute("type", "button");
                let _ = b.set_attribute("data-role", role);
            }
        }
        // one listener for the sliders, one for the buttons
        let shared = self.shared.clone();
        let root = panel.clone();
        let start = self.alignment;
        let on_input = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let read = |key: &str, fallback: f32| {
                root.query_selector(&format!("[data-field={key}]"))
                    .ok()
                    .flatten()
                    .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
                    .and_then(|i| i.value().parse::<f32>().ok())
                    .unwrap_or(fallback)
            };
            shared.borrow_mut().alignment = Some(Alignment {
                hfov_deg: read("hfov_deg", start.hfov_deg),
                yaw_deg: read("yaw_deg", start.yaw_deg),
                pitch_deg: read("pitch_deg", start.pitch_deg),
                roll_deg: read("roll_deg", start.roll_deg),
                dx: read("dx", start.dx),
                dy: read("dy", start.dy),
                dz: read("dz", start.dz),
                lag_s: read("lag_s", start.lag_s),
            });
        });
        let _ = panel.add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref());
        on_input.forget();
        let shared = self.shared.clone();
        let on_click = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let role = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .and_then(|t| t.get_attribute("data-role"));
            match role.as_deref() {
                Some("save") => shared.borrow_mut().save = true,
                Some("leave") => shared.borrow_mut().leave = true,
                _ => {}
            }
        });
        let _ = panel.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref());
        on_click.forget();
        if let Some(body) = d.body() {
            let _ = body.append_child(&panel);
        }
        self.panel = Some(panel);
    }

    pub fn close(&mut self, gl: &Gl) {
        if let Some(p) = self.panel.take() {
            p.remove();
        }
        if let Some(m) = self.bare.take() {
            gl.delete_mesh(m);
        }
        for (_, m) in self.pictures.drain(..) {
            gl.delete_mesh(m);
        }
        for (_, t) in std::mem::take(&mut self.textures) {
            gl.delete_texture(t);
        }
        for (_, b) in std::mem::take(&mut self.bitmaps) {
            b.close();
        }
    }
}
