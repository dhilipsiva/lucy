//! The garage in the browser: its scenes, the XR sessions, controllers, haptics, sound and the
//! speedometer. Hit-test, planes and haptics have no web-sys bindings yet, so they are read
//! with `Reflect`, as xr-check does.

use super::homecap::{Capturing, Viewing, Walk};
use super::{car, drive, launch, planes, Mode};
use crate::app::App;
use crate::colors::linear;
use crate::geom;
use crate::gl::{Blend, Gl, GpuMesh, Lights};
use crate::scene::{Kind, Material, Node, NodeId, Scene};
use crate::sources::js_error;
use glam::{Mat4, Quat, Vec2, Vec3};
use js_sys::{Array, Function, Object, Reflect};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    CanvasRenderingContext2d, GamepadButton, HtmlCanvasElement, WebGl2RenderingContext, XrFrame,
    XrInputSource, XrInputSourceEvent, XrReferenceSpace, XrReferenceSpaceType, XrRenderStateInit,
    XrSession, XrSessionInit, XrSessionMode, XrWebGlLayer,
};

/// Where the driver's eyes are in the cockpit model; the cockpit is moved to his real head.
const EYE: f32 = 1.2;
const SKY: u32 = 0x8FA7BF;

pub(super) fn get(v: &JsValue, key: &str) -> JsValue {
    Reflect::get(v, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
pub(super) fn present(v: &JsValue) -> bool {
    !(v.is_undefined() || v.is_null())
}
pub(super) fn call(obj: &JsValue, name: &str, args: &Array) -> Result<JsValue, String> {
    let f: Function = get(obj, name)
        .dyn_into()
        .map_err(|_| format!("{name} is not available"))?;
    Reflect::apply(&f, obj, args).map_err(|e| js_error(&e))
}
/// A pose's transform as a matrix (an XRPose read without bindings).
pub(super) fn pose_matrix(v: &JsValue) -> Option<Mat4> {
    matrix(v)
}
fn matrix(v: &JsValue) -> Option<Mat4> {
    let m = get(&get(v, "transform"), "matrix");
    if !present(&m) {
        return None;
    }
    let a: Vec<f32> = js_sys::Float32Array::new(&m).to_vec();
    let a: [f32; 16] = a.try_into().ok()?;
    Some(Mat4::from_cols_array(&a))
}
fn mirror(text: &str) {
    if let Some(e) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("garage-status"))
    {
        e.set_text_content(Some(text));
    }
}

/// A synthesised electric whine: two oscillators through a low-pass filter. Mine, not a maker's.
pub struct Audio {
    ctx: web_sys::AudioContext,
    saw: web_sys::OscillatorNode,
    sub: web_sys::OscillatorNode,
    filter: web_sys::BiquadFilterNode,
    gain: web_sys::GainNode,
}
impl Audio {
    /// Created inside the visitor's click, so the browser lets it play.
    pub fn new() -> Result<Audio, String> {
        let e = |e: JsValue| js_error(&e);
        let ctx = web_sys::AudioContext::new().map_err(e)?;
        let saw = ctx.create_oscillator().map_err(e)?;
        saw.set_type(web_sys::OscillatorType::Sawtooth);
        let sub = ctx.create_oscillator().map_err(e)?;
        sub.set_type(web_sys::OscillatorType::Sine);
        let filter = ctx.create_biquad_filter().map_err(e)?;
        filter.set_type(web_sys::BiquadFilterType::Lowpass);
        let gain = ctx.create_gain().map_err(e)?;
        gain.gain().set_value(0.0);
        saw.connect_with_audio_node(&filter).map_err(e)?;
        sub.connect_with_audio_node(&filter).map_err(e)?;
        filter.connect_with_audio_node(&gain).map_err(e)?;
        gain.connect_with_audio_node(&ctx.destination())
            .map_err(e)?;
        saw.start().map_err(e)?;
        sub.start().map_err(e)?;
        Ok(Audio {
            ctx,
            saw,
            sub,
            filter,
            gain,
        })
    }
    /// `pitch` 0..1 follows speed, `load` 0..1 follows acceleration.
    fn set(&self, pitch: f32, load: f32, on: bool) {
        let t = self.ctx.current_time();
        let f = 70.0 + 900.0 * pitch;
        let _ = self.saw.frequency().set_target_at_time(f, t, 0.05);
        let _ = self.sub.frequency().set_target_at_time(f / 2.0, t, 0.05);
        let _ = self
            .filter
            .frequency()
            .set_target_at_time(500.0 + 3500.0 * pitch, t, 0.05);
        let level = if on && (pitch > 0.001 || load > 0.01) {
            0.03 + 0.12 * load.max(pitch * 0.4)
        } else {
            0.0
        };
        let _ = self.gain.gain().set_target_at_time(level, t, 0.08);
    }
    fn close(&self) {
        let _ = self.ctx.close();
    }
}

/// A canvas drawn with text and uploaded as a board's texture.
struct Board {
    canvas: HtmlCanvasElement,
    texture: usize,
    node: NodeId,
    text: String,
}
impl Board {
    fn new(gl: &Gl, scene: &mut Scene, parent: NodeId, w: f32, h: f32, px: (u32, u32)) -> Board {
        let doc = web_sys::window().unwrap().document().unwrap();
        let canvas: HtmlCanvasElement = doc.create_element("canvas").unwrap().unchecked_into();
        canvas.set_width(px.0);
        canvas.set_height(px.1);
        let texture = scene.add_texture(gl.texture_from_canvas(&canvas, true));
        let node = scene.add(
            parent,
            Node::new(Kind::Board {
                texture,
                w,
                h,
                opacity: 1.0,
            }),
        );
        Board {
            canvas,
            texture,
            node,
            text: String::new(),
        }
    }
    /// Lines of (size in canvas pixels, text); redrawn only when they change.
    fn draw(&mut self, gl: &Gl, scene: &Scene, lines: &[(f64, String)]) {
        let key: String = lines
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if key == self.text {
            return;
        }
        self.text = key;
        let Some(ctx) = self
            .canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
        else {
            return;
        };
        let (w, h) = (self.canvas.width() as f64, self.canvas.height() as f64);
        ctx.clear_rect(0.0, 0.0, w, h);
        ctx.set_fill_style_str("rgba(12,14,18,0.82)");
        ctx.begin_path();
        let _ = ctx.round_rect_with_f64(0.0, 0.0, w, h, 28.0);
        ctx.fill();
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let total: f64 = lines.iter().map(|(s, _)| s * 1.25).sum();
        let mut y = (h - total) / 2.0;
        for (size, text) in lines {
            y += size * 0.625;
            ctx.set_fill_style_str("#F4F3EF");
            ctx.set_font(&crate::panel::font(600, *size, crate::panel::DISPLAY));
            let _ = ctx.fill_text(text, w / 2.0, y);
            y += size * 0.625;
        }
        gl.update_texture(&scene.textures[self.texture], &self.canvas, true);
    }
}

/// What the XR side of the garage needs from the session.
pub struct XrState {
    pub session: XrSession,
    pub layer: XrWebGlLayer,
    pub space: XrReferenceSpace,
    pub floor: bool,
    hit: Rc<RefCell<HashMap<String, JsValue>>>,
    asked: HashSet<String>,
}

#[derive(Default)]
struct Pad {
    pressed: Vec<bool>,
    values: Vec<f32>,
    axes: Vec<f32>,
}

pub struct Garage {
    pub mode: Mode,
    scene: Scene,
    track: NodeId,
    road: NodeId,
    car: NodeId,
    reticle: NodeId,
    board: Board,
    vignette_mesh: GpuMesh,
    pressed_at: Option<f64>,
    vignette: bool,
    vignette_now: f32,
    sound: bool,
    drive: drive::Car,
    car_y: f32,
    placed: bool,
    walls: Vec<drive::Segment>,
    targets: HashMap<String, Vec3>,
    head: Mat4,
    seated: bool,
    pads: HashMap<String, Pad>,
    prev: HashMap<String, Vec<bool>>,
    snap: crate::xr::Snap,
    bump_at: f64,
    frames: u64,
    keys_down: HashSet<String>,
    pub xr: Option<XrState>,
    pub audio: Option<Audio>,
    pub leave: bool,
    pub back_to_chat: bool,
    status: String,
    /// A capture of his home in progress, or one being looked at (roadmap/N8j.md).
    pub cap: Option<Capturing>,
    pub view: Option<Viewing>,
    drag: Option<(f64, f64)>,
    /// What to tell the visitor once the garage has closed.
    pub note: Option<String>,
}

fn mesh_node(gl: &Gl, scene: &mut Scene, parent: NodeId, m: &geom::Mesh, mat: Material) -> NodeId {
    let mesh = scene.add_mesh(gl.upload(m));
    scene.add(
        parent,
        Node::new(Kind::Mesh {
            mesh,
            material: mat,
        }),
    )
}

impl Garage {
    pub fn new(gl: &Gl, mode: Mode, back_to_chat: bool) -> Garage {
        let ar = mode.ar();
        let mut scene = Scene::new(
            gl,
            Lights {
                hemi_sky: linear(0xC9D6E6),
                hemi_ground: linear(0x3A3530),
                hemi_intensity: if ar { 1.1 } else { 0.9 },
                dir_color: linear(0xFFF4E6),
                dir_dir: Vec3::new(0.4, 1.0, 0.6).normalize(),
                dir_intensity: 1.4,
                points: vec![],
                fog_color: linear(SKY),
                // no fog in the visitor's own room
                fog_near: if ar { 1.0e6 } else { 120.0 },
                fog_far: if ar { 2.0e6 } else { 520.0 },
            },
        );
        let root = scene.root;
        let track = scene.add(root, Node::new(Kind::Empty));
        let road = scene.add(track, Node::new(Kind::Empty));
        if mode == Mode::Launch {
            build_road(gl, &mut scene, road);
            build_cockpit(gl, &mut scene, track);
        }
        // the car itself, for the two modes in the visitor's room
        let car_node = scene.add(root, Node::new(Kind::Empty));
        for (part, m) in car::parts(1.0) {
            let mat = match part {
                car::Part::Body => Material::std(0x41536A).rough(0.3),
                car::Part::Cabin => Material::std(0x141A22).rough(0.12),
                car::Part::Wheel => Material::std(0x17181A).rough(0.9),
                car::Part::Headlight => Material::std(0xFFF6E0).glow(2.0),
                car::Part::Taillight => Material::std(0xFF3030).glow(1.5),
            };
            mesh_node(gl, &mut scene, car_node, &m, mat);
        }
        scene.set_visible(car_node, false);
        let reticle = mesh_node(
            gl,
            &mut scene,
            root,
            &geom::ring(0.07, 0.09, 40),
            Material::std(0xF2A33A).glow(1.5).two_sided(),
        );
        scene.node(reticle).rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        scene.set_visible(reticle, false);
        let board = if mode == Mode::Launch {
            // on the centre console, clear of the wheel, turned to the driver
            let b = Board::new(gl, &mut scene, track, 0.3, 0.15, (1024, 512));
            scene.set_pos(b.node, Vec3::new(0.36, EYE - 0.24, -0.5));
            scene.node(b.node).rotation =
                Quat::from_rotation_y(-0.62) * Quat::from_rotation_x(-0.2);
            b
        } else {
            let b = Board::new(gl, &mut scene, root, 0.6, 0.15, (1024, 256));
            scene.set_visible(b.node, false);
            b
        };
        let vignette_mesh = gl.upload(&geom::ring(1.0, 6.0, 64));
        Garage {
            mode,
            scene,
            track,
            road,
            car: car_node,
            reticle,
            board,
            vignette_mesh,
            pressed_at: None,
            vignette: true,
            vignette_now: 0.0,
            sound: true,
            drive: drive::Car::new(Vec2::ZERO, 0.0, mode.scale()),
            car_y: 0.0,
            placed: false,
            walls: vec![],
            targets: HashMap::new(),
            head: Mat4::from_translation(Vec3::new(0.0, EYE, 0.0)),
            seated: false,
            pads: HashMap::new(),
            prev: HashMap::new(),
            snap: crate::xr::Snap::default(),
            bump_at: 0.0,
            frames: 0,
            keys_down: HashSet::new(),
            xr: None,
            audio: None,
            leave: false,
            back_to_chat,
            status: String::new(),
            cap: None,
            view: (mode == Mode::Home).then(Viewing::new),
            drag: None,
            note: None,
        }
    }

    fn set_status(&mut self, s: String) {
        if s != self.status {
            mirror(&s);
            self.status = s;
        }
    }

    /// The trigger, a pinch or Space: launch, or place the car where the ring is.
    pub fn press(&mut self, now: f64, hand: &str) {
        match self.mode {
            Mode::Launch => {
                let idle = match self.pressed_at {
                    None => true,
                    Some(t) => {
                        launch::sample(((now - t) / 1000.0) as f32).phase == launch::Phase::Stopped
                    }
                };
                if idle {
                    self.pressed_at = Some(now);
                }
            }
            Mode::LifeSize => self.place(hand),
            Mode::Miniature if !self.placed => self.place(hand),
            Mode::Miniature | Mode::Capture | Mode::Home => {}
        }
    }

    fn place(&mut self, hand: &str) {
        let target = self
            .targets
            .get(hand)
            .or_else(|| self.targets.get("right"))
            .or_else(|| self.targets.values().next())
            .copied()
            .unwrap_or_else(|| {
                // no hit-test: on the floor straight ahead
                let head = self.head.w_axis.truncate();
                let mut f = self.head.transform_vector3(-Vec3::Z);
                f.y = 0.0;
                let f = f.normalize_or(Vec3::NEG_Z);
                let d = if self.mode == Mode::Miniature {
                    0.8
                } else {
                    3.2
                };
                let floor = if self.xr.as_ref().is_some_and(|x| x.floor) {
                    0.0
                } else {
                    head.y - 1.6
                };
                Vec3::new(head.x + f.x * d, floor, head.z + f.z * d)
            });
        let head = self.head.w_axis.truncate();
        let to_me = Vec2::new(head.x - target.x, head.z - target.z).normalize_or(Vec2::NEG_Y);
        let yaw = (-to_me.x).atan2(-to_me.y);
        self.drive = drive::Car::new(Vec2::new(target.x, target.z), yaw, self.mode.scale());
        self.car_y = target.y;
        self.placed = true;
    }

    /// Edge-triggered buttons per hand (xr-standard: 0 trigger, 1 grip, 3 stick, 4 A/X, 5 B/Y).
    fn clicked(&self, hand: &str, i: usize) -> bool {
        let now = self
            .pads
            .get(hand)
            .and_then(|p| p.pressed.get(i))
            .copied()
            .unwrap_or(false);
        let before = self
            .prev
            .get(hand)
            .and_then(|p| p.get(i))
            .copied()
            .unwrap_or(false);
        now && !before
    }
    fn value(&self, hand: &str, i: usize) -> f32 {
        self.pads
            .get(hand)
            .and_then(|p| p.values.get(i))
            .copied()
            .unwrap_or(0.0)
    }
    fn axis(&self, hand: &str, i: usize) -> f32 {
        self.pads
            .get(hand)
            .and_then(|p| p.axes.get(i))
            .copied()
            .unwrap_or(0.0)
    }

    /// One step of the garage's own logic, shared by the headset and the flat screen.
    fn step(&mut self, gl: &Gl, now: f64, dt: f32) -> (f32, Vec<(String, f32, f64)>) {
        let mut pulses = vec![];
        let mut accel_frac = 0.0;
        match self.mode {
            // these two run in their own frame paths below
            Mode::Capture | Mode::Home => {}
            Mode::Launch => {
                if self.clicked("right", 4) {
                    self.pressed_at = None;
                }
                if self.clicked("left", 4) {
                    self.sound = !self.sound;
                }
                if self.clicked("left", 5) {
                    self.vignette = !self.vignette;
                }
                let s = match self.pressed_at {
                    Some(t) => launch::sample(((now - t) / 1000.0) as f32),
                    None => launch::sample(0.0),
                };
                let running = self.pressed_at.is_some();
                let distance = if running { s.distance } else { 0.0 };
                self.scene.set_pos(self.road, Vec3::new(0.0, 0.0, distance));
                accel_frac = (s.accel.abs() / launch::accel()).min(1.0);
                if running && s.phase == launch::Phase::Launch {
                    let i = 0.25 + 0.6 * (s.t / launch::TARGET_S);
                    pulses.push(("left".into(), i, 30.0));
                    pulses.push(("right".into(), i, 30.0));
                } else if running && s.phase == launch::Phase::Brake {
                    pulses.push(("left".into(), 0.15, 30.0));
                    pulses.push(("right".into(), 0.15, 30.0));
                }
                if let Some(a) = &self.audio {
                    a.set(
                        s.speed / launch::top(),
                        if s.accel > 0.0 { 1.0 } else { 0.3 },
                        self.sound && running,
                    );
                }
                let big = if !running {
                    "READY".to_string()
                } else if s.phase == launch::Phase::Countdown {
                    s.count.to_string()
                } else {
                    format!("{:.0} km/h", s.kmh())
                };
                let mid = if running && s.phase != launch::Phase::Countdown {
                    format!("{:.2} s", s.t.min(99.0))
                } else {
                    "0–100 km/h in 2.5 s".into()
                };
                let hint = if self.xr.is_some() {
                    "Trigger: launch · A: again · Y: comfort · X: sound · B: leave"
                } else {
                    "Space: launch · R: again · V: comfort · M: sound · Esc: leave"
                };
                let lines = vec![
                    (150.0, big.clone()),
                    (64.0, mid.clone()),
                    (
                        40.0,
                        "average 1.13 g · VR can't push you into the seat".into(),
                    ),
                    (34.0, hint.into()),
                ];
                self.board.draw(gl, &self.scene, &lines);
                self.set_status(format!(
                    "garage launch · {} · {} · {:?} · {:.2} m",
                    big, mid, s.phase, distance
                ));
            }
            Mode::LifeSize | Mode::Miniature => {
                if self.mode == Mode::LifeSize && self.placed {
                    let turn = self.snap.turn(self.axis("right", 2), now) / 2.0;
                    self.drive.yaw += turn;
                }
                if self.mode == Mode::Miniature && self.clicked("right", 4) {
                    self.placed = false;
                    self.press(now, "right");
                }
                if self.mode == Mode::Miniature && self.placed {
                    let controls = drive::Controls {
                        throttle: self.value("right", 0).max(-self.axis("left", 3)).max(0.0),
                        brake: self.value("left", 0).max(self.axis("left", 3)).max(0.0),
                        steer: self.axis("left", 2),
                    };
                    let hit = self.drive.step(controls, dt, &self.walls);
                    if hit && now - self.bump_at > 150.0 {
                        self.bump_at = now;
                        pulses.push(("right".into(), 0.8, 60.0));
                    }
                    accel_frac = controls.throttle.max(controls.brake);
                    if let Some(a) = &self.audio {
                        a.set(
                            (self.drive.speed.abs() / 3.0).min(1.0),
                            accel_frac,
                            self.sound,
                        );
                    }
                }
                let scale = self.mode.scale();
                self.scene.set_visible(self.car, self.placed);
                self.scene.set_pos(
                    self.car,
                    Vec3::new(self.drive.pos.x, self.car_y, self.drive.pos.y),
                );
                self.scene.node(self.car).rotation = Quat::from_rotation_y(self.drive.yaw);
                self.scene.set_scale(self.car, Vec3::splat(scale));
                let target = self
                    .targets
                    .get("right")
                    .or_else(|| self.targets.values().next())
                    .copied();
                let show_ring = target.is_some() && !(self.mode == Mode::Miniature && self.placed);
                self.scene.set_visible(self.reticle, show_ring);
                if let Some(t) = target {
                    self.scene.set_pos(self.reticle, t + Vec3::Y * 0.003);
                    self.scene.set_scale(
                        self.reticle,
                        Vec3::splat(if scale < 1.0 { 0.6 } else { 2.0 }),
                    );
                }
                let text = match (self.mode, self.placed) {
                    (Mode::LifeSize, false) => "Point at the floor and pull the trigger · B: leave",
                    (Mode::LifeSize, true) => "Walk around it · trigger: move · right stick: turn · B: leave",
                    (Mode::Miniature, false) => "Point at the floor and pull the trigger · B: leave",
                    (Mode::Miniature, true) => "Right trigger: go · left trigger: brake · left stick: steer · A: place again",
                    _ => "",
                };
                self.board.draw(gl, &self.scene, &[(56.0, text.into())]);
                let head = self.head.w_axis.truncate();
                let board_at = if self.placed {
                    Vec3::new(
                        self.drive.pos.x,
                        self.car_y + (car::HEIGHT + 0.35) * scale + 0.1,
                        self.drive.pos.y,
                    )
                } else {
                    head + self.head.transform_vector3(Vec3::new(0.0, -0.25, -1.1))
                };
                self.scene.set_visible(self.board.node, true);
                self.scene.set_pos(self.board.node, board_at);
                let face = Vec2::new(head.x - board_at.x, head.z - board_at.z);
                self.scene.node(self.board.node).rotation =
                    Quat::from_rotation_y(face.x.atan2(face.y));
                self.set_status(format!(
                    "garage {:?} · placed {} · {:.2},{:.2} · speed {:.2} · walls {}",
                    self.mode,
                    self.placed,
                    self.drive.pos.x,
                    self.drive.pos.y,
                    self.drive.speed,
                    self.walls.len()
                ));
            }
        }
        let target = if self.vignette { accel_frac } else { 0.0 };
        self.vignette_now += (target - self.vignette_now) * (dt * 6.0).min(1.0);
        (self.vignette_now, pulses)
    }

    fn draw_vignette(&self, gl: &Gl, proj: &Mat4, strength: f32) {
        if strength < 0.02 {
            return;
        }
        // a dark ring locked to the eye, its hole narrowing as the acceleration grows
        let inner = 0.36 - 0.16 * strength;
        let model = Mat4::from_translation(Vec3::new(0.0, 0.0, -0.3))
            * Mat4::from_scale(Vec3::splat(inner));
        gl.draw_flat(
            &self.vignette_mesh,
            &(*proj * model),
            None,
            [0.0, 0.0, 0.0, 0.9 * strength],
            Blend::Alpha,
            false,
            false,
        );
    }

    // ── the flat screen ──
    pub fn key(&mut self, key: &str, down: bool, now: f64) {
        if !down {
            self.keys_down.remove(key);
            return;
        }
        if !self.keys_down.insert(key.to_string()) {
            return;
        }
        if self.mode == Mode::Home {
            // the walking keys are read while held, in frame_flat
            if key == "escape" {
                self.leave = true;
            }
            return;
        }
        match key {
            " " | "spacebar" => self.press(now, "right"),
            "r" => self.pressed_at = None,
            "v" => self.vignette = !self.vignette,
            "m" => self.sound = !self.sound,
            "escape" => self.leave = true,
            _ => {}
        }
    }

    /// Drag to look around a captured home on a flat screen.
    pub fn pointer(&mut self, kind: &str, x: f64, y: f64) {
        match kind {
            "down" => self.drag = Some((x, y)),
            "move" => {
                if let (Some((px, py)), Some(v)) = (self.drag, self.view.as_mut()) {
                    v.yaw -= (x - px) as f32 * 0.004;
                    v.pitch = (v.pitch - (y - py) as f32 * 0.004).clamp(-1.4, 1.4);
                    self.drag = Some((x, y));
                }
            }
            _ => self.drag = None,
        }
    }

    fn home_flat(&mut self, gl: &Gl, now: f64, dt: f32, w: f64, h: f64) {
        let held = |keys: &[&str]| keys.iter().any(|k| self.keys_down.contains(*k));
        let axis = |plus: &[&str], minus: &[&str]| {
            f32::from(u8::from(held(plus))) - f32::from(u8::from(held(minus)))
        };
        let walk = Walk {
            forward: axis(&["w", "arrowup"], &["s", "arrowdown"]),
            strafe: axis(&["d"], &["a"]),
            turn: axis(&["arrowleft", "q"], &["arrowright", "e"]) * 1.4,
            look: 0.0,
            rise: axis(&["r"], &["f"]),
        };
        let Some(v) = self.view.as_mut() else { return };
        v.step(gl, now, dt, walk, true);
        if v.leave {
            self.leave = true;
        }
        if let Some(e) = &v.error {
            self.note = Some(e.clone());
            self.leave = true;
        }
        let world = Mat4::from_translation(v.pos)
            * Mat4::from_rotation_y(v.yaw)
            * Mat4::from_rotation_x(v.pitch);
        let proj =
            Mat4::perspective_rh_gl(70f32.to_radians(), (w / h.max(1.0)) as f32, 0.05, 200.0);
        gl.clear(sky());
        v.render(gl, &world.inverse(), &proj, v.pos);
        let status = v.status();
        self.set_status(status);
    }

    pub fn frame_flat(&mut self, gl: &Gl, now: f64, dt: f32, w: f64, h: f64) {
        if self.mode == Mode::Home {
            self.frames += 1;
            return self.home_flat(gl, now, dt, w, h);
        }
        if self.frames == 0 {
            // the vignette is a headset comfort aid; on a flat screen it starts off (V turns it on)
            self.vignette = false;
        }
        self.frames += 1;
        let (vignette, _) = self.step(gl, now, dt);
        let eye = Vec3::new(0.0, EYE, 0.0);
        let view = Mat4::look_to_rh(eye, Vec3::new(0.0, -0.08, -1.0).normalize(), Vec3::Y);
        let proj =
            Mat4::perspective_rh_gl(70f32.to_radians(), (w / h.max(1.0)) as f32, 0.05, 1200.0);
        gl.clear(sky());
        self.scene.render(gl, &view, &proj, eye);
        self.draw_vignette(gl, &proj, vignette);
    }

    // ── the headset ──
    fn read_pads(&mut self, session: &XrSession) {
        self.prev = self
            .pads
            .iter()
            .map(|(k, p)| (k.clone(), p.pressed.clone()))
            .collect();
        self.pads.clear();
        let sources = session.input_sources();
        for i in 0..sources.length() {
            let Some(source) = sources.get(i) else {
                continue;
            };
            let Some(pad) = source.gamepad() else {
                continue;
            };
            let mut p = Pad::default();
            for b in pad.buttons().iter() {
                let b: GamepadButton = b.unchecked_into();
                p.pressed.push(b.pressed());
                p.values.push(b.value() as f32);
            }
            let dead = |v: f32| if v.abs() > 0.15 { v } else { 0.0 };
            p.axes = pad
                .axes()
                .iter()
                .map(|a| dead(a.as_f64().unwrap_or(0.0) as f32))
                .collect();
            self.pads.insert(hand_of(&source), p);
        }
    }

    fn hit_test(&mut self, frame: &XrFrame) {
        let Some(xr) = &mut self.xr else { return };
        let sources = xr.session.input_sources();
        for i in 0..sources.length() {
            let Some(source) = sources.get(i) else {
                continue;
            };
            let hand = hand_of(&source);
            if !xr.asked.contains(&hand) {
                xr.asked.insert(hand.clone());
                let options = Object::new();
                let _ = Reflect::set(&options, &"space".into(), &source.target_ray_space());
                if let Ok(p) = call(&xr.session, "requestHitTestSource", &Array::of1(&options)) {
                    let store = xr.hit.clone();
                    let key = hand.clone();
                    spawn_local(async move {
                        if let Ok(src) = JsFuture::from(js_sys::Promise::from(p)).await {
                            store.borrow_mut().insert(key, src);
                        }
                    });
                }
            }
        }
        self.targets.clear();
        for (hand, src) in xr.hit.borrow().iter() {
            let Ok(results) = call(frame, "getHitTestResults", &Array::of1(src)) else {
                continue;
            };
            let first = Array::from(&results).get(0);
            if !present(&first) {
                continue;
            }
            let Ok(pose) = call(&first, "getPose", &Array::of1(&xr.space)) else {
                continue;
            };
            if let Some(m) = matrix(&pose) {
                self.targets.insert(hand.clone(), m.w_axis.truncate());
            }
        }
    }

    fn read_planes(&mut self, frame: &XrFrame) {
        let Some(xr) = &self.xr else { return };
        let set = get(frame, "detectedPlanes");
        if !present(&set) {
            return;
        }
        let Ok(Some(items)) = js_sys::try_iter(&set) else {
            return;
        };
        let floor = if xr.floor {
            0.0
        } else {
            self.head.w_axis.y - 1.6
        };
        let mut walls = vec![];
        for plane in items.flatten() {
            let orientation = match get(&plane, "orientation").as_string().as_deref() {
                Some("vertical") => planes::Orientation::Vertical,
                _ => planes::Orientation::Horizontal,
            };
            let Ok(pose) = call(
                frame,
                "getPose",
                &Array::of2(&get(&plane, "planeSpace"), &xr.space),
            ) else {
                continue;
            };
            let Some(m) = matrix(&pose) else { continue };
            let polygon: Vec<Vec3> = Array::from(&get(&plane, "polygon"))
                .iter()
                .map(|p| {
                    let f = |k| get(&p, k).as_f64().unwrap_or(0.0) as f32;
                    Vec3::new(f("x"), f("y"), f("z"))
                })
                .collect();
            walls.extend(planes::obstacles(m, &polygon, orientation, floor));
        }
        self.walls = walls;
    }

    fn pulse(&self, session: &XrSession, pulses: &[(String, f32, f64)]) {
        if pulses.is_empty() {
            return;
        }
        let sources = session.input_sources();
        for i in 0..sources.length() {
            let Some(source) = sources.get(i) else {
                continue;
            };
            let hand = hand_of(&source);
            let Some((_, intensity, ms)) = pulses.iter().find(|p| p.0 == hand) else {
                continue;
            };
            let Some(pad) = source.gamepad() else {
                continue;
            };
            let actuator = Array::from(&get(&pad, "hapticActuators")).get(0);
            if present(&actuator) {
                let _ = call(
                    &actuator,
                    "pulse",
                    &Array::of2(&(*intensity as f64).into(), &(*ms).into()),
                );
            }
        }
    }

    /// One headset frame: input, logic, then both eyes.
    pub fn frame_xr(&mut self, gl: &Gl, now: f64, dt: f32, frame: &XrFrame) {
        let Some(xr) = &self.xr else { return };
        let (session, layer, space) = (xr.session.clone(), xr.layer.clone(), xr.space.clone());
        let Some(pose) = frame.get_viewer_pose(&space) else {
            return;
        };
        self.head = crate::xr::matrix(&pose.transform().matrix());
        self.frames += 1;
        if self.mode == Mode::Launch && !self.seated {
            // move the cockpit to where he sits, facing where he looks
            let head = self.head.w_axis.truncate();
            let f = self.head.transform_vector3(-Vec3::Z);
            let yaw = (-f.x).atan2(-f.z);
            self.scene
                .set_pos(self.track, Vec3::new(head.x, head.y - EYE, head.z));
            self.scene.node(self.track).rotation = Quat::from_rotation_y(yaw);
            self.seated = true;
        }
        self.read_pads(&session);
        if self.clicked("right", 5) {
            self.leave = true;
        }
        if matches!(self.mode, Mode::LifeSize | Mode::Miniature) {
            self.hit_test(frame);
            if self.frames % 10 == 1 {
                self.read_planes(frame);
            }
        }
        if self.mode == Mode::Capture {
            let floor = self.xr.as_ref().is_some_and(|x| x.floor);
            let head = self.head;
            if let Some(cap) = self.cap.as_mut() {
                let lines = cap.frame(now, dt, frame, &space, floor, head);
                let status = cap.status();
                self.board.draw(gl, &self.scene, &lines);
                // the board floats ahead of him, a little below his eyes, turned to face him
                let at = head.w_axis.truncate();
                let mut f = head.transform_vector3(-Vec3::Z);
                f.y = 0.0;
                let f = f.normalize_or(Vec3::NEG_Z);
                let spot = at + f * 1.2 - Vec3::Y * 0.35;
                self.scene.set_visible(self.board.node, true);
                self.scene.set_pos(self.board.node, spot);
                self.scene.node(self.board.node).rotation =
                    Quat::from_rotation_y((-f.x).atan2(-f.z));
                self.set_status(status);
            }
        }
        if self.mode == Mode::Home {
            // thumbsticks: the left walks the way he faces, the right turns in steps
            let f = self.head.transform_vector3(-Vec3::Z);
            let head_yaw = (-f.x).atan2(-f.z);
            let stick = Vec3::new(self.axis("left", 2), 0.0, self.axis("left", 3));
            let moved = Quat::from_rotation_y(head_yaw) * stick;
            let turn = self.snap.turn(self.axis("right", 2), now);
            if let Some(v) = self.view.as_mut() {
                v.yaw += turn;
                v.step(
                    gl,
                    now,
                    dt,
                    Walk {
                        forward: -moved.z,
                        strafe: moved.x,
                        ..Default::default()
                    },
                    false,
                );
                if let Some(e) = &v.error {
                    self.note = Some(e.clone());
                    self.leave = true;
                }
            }
            let status = self.view.as_ref().map(|v| v.status()).unwrap_or_default();
            self.set_status(status);
        }
        let (vignette, pulses) = self.step(gl, now, dt);
        self.pulse(&session, &pulses);
        let g = &gl.ctx;
        g.bind_framebuffer(
            WebGl2RenderingContext::FRAMEBUFFER,
            layer.framebuffer().as_ref(),
        );
        gl.viewport(
            0,
            0,
            layer.framebuffer_width() as i32,
            layer.framebuffer_height() as i32,
        );
        if self.mode.ar() {
            // transparent: his own room shows through
            g.clear_color(0.0, 0.0, 0.0, 0.0);
            g.clear(
                WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT,
            );
        } else {
            gl.clear(sky());
        }
        for value in pose.views().iter() {
            let view: web_sys::XrView = value.unchecked_into();
            let Some(vp) = layer.get_viewport(&view) else {
                continue;
            };
            gl.viewport(vp.x(), vp.y(), vp.width(), vp.height());
            let mut eye_world = crate::xr::matrix(&view.transform().matrix());
            let proj = crate::xr::matrix(&view.projection_matrix());
            if let Some(v) = self.view.as_ref() {
                // he stands in the captured home: its floor under his own
                eye_world = Mat4::from_rotation_translation(
                    Quat::from_rotation_y(v.yaw),
                    Vec3::new(v.pos.x, 0.0, v.pos.z),
                ) * eye_world;
                v.render(gl, &eye_world.inverse(), &proj, eye_world.w_axis.truncate());
                continue;
            }
            self.scene
                .render(gl, &eye_world.inverse(), &proj, eye_world.w_axis.truncate());
            if self.mode == Mode::Launch {
                self.draw_vignette(gl, &proj, vignette);
            }
        }
        if self.leave {
            let _ = session.end();
        }
    }

    pub fn select(&mut self, now: f64, event: &XrInputSourceEvent) {
        let hand = hand_of(&event.input_source());
        self.press(now, &hand);
    }

    /// A capture that was running ends here: camera off, and what it gathered handed over.
    pub fn finish_capture(
        &mut self,
    ) -> Option<(
        lucy_room_memory::home::HomeCapture,
        Vec<(u32, web_sys::Blob)>,
    )> {
        self.cap.take().map(|c| c.finish())
    }

    pub fn close(&mut self, gl: &Gl) {
        if let Some(a) = self.audio.take() {
            a.close();
        }
        // a capture nobody collected still has its camera stopped
        drop(self.finish_capture());
        if let Some(mut v) = self.view.take() {
            v.close(gl);
        }
        mirror("");
    }
}

fn hand_of(source: &XrInputSource) -> String {
    match source.handedness() {
        web_sys::XrHandedness::Left => "left",
        web_sys::XrHandedness::Right => "right",
        _ => "none",
    }
    .into()
}
fn sky() -> [f32; 3] {
    // sRGB already, as Gl::clear expects
    let c = SKY;
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
    ]
}

/// A long straight road with lane marks and posts every ten metres, so speed reads at the edge
/// of sight. Built ahead of the driver along −Z; it slides toward him as the car goes.
fn build_road(gl: &Gl, scene: &mut Scene, road: NodeId) {
    let length = launch::total_distance() + 400.0;
    let ground = geom::plane(4000.0, 4000.0);
    let g = mesh_node(gl, scene, road, &ground, Material::std(0x2F3A2C).rough(1.0));
    scene.node(g).rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    scene.set_pos(g, Vec3::new(0.0, -0.02, -length / 2.0));
    let asphalt = geom::plane(12.0, length + 60.0);
    let a = mesh_node(
        gl,
        scene,
        road,
        &asphalt,
        Material::std(0x2A2C30).rough(0.95),
    );
    scene.node(a).rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    scene.set_pos(a, Vec3::new(0.0, 0.0, -length / 2.0 + 30.0));
    let mut marks = geom::Mesh::default();
    let mut posts = geom::Mesh::default();
    let mut z = 0.0;
    while z < length {
        marks.append(&geom::cuboid(0.15, 0.01, 3.0).translate(Vec3::new(0.0, 0.005, -z - 1.5)));
        for x in [-7.0f32, 7.0] {
            posts.append(&geom::cylinder(0.08, 0.08, 1.4, 8).translate(Vec3::new(x, 0.7, -z)));
        }
        z += 10.0;
    }
    for x in [-5.6f32, 5.6] {
        marks.append(&geom::cuboid(0.15, 0.01, length).translate(Vec3::new(
            x,
            0.005,
            -length / 2.0,
        )));
    }
    marks.append(&geom::cuboid(11.2, 0.012, 0.5).translate(Vec3::new(0.0, 0.006, 0.0)));
    mesh_node(gl, scene, road, &marks, Material::std(0xE8E6E0).glow(0.3));
    mesh_node(gl, scene, road, &posts, Material::std(0xF2A33A).glow(0.4));
}

/// The fixed frame around the driver: the strongest known help against VR sickness.
fn build_cockpit(gl: &Gl, scene: &mut Scene, track: NodeId) {
    let body = Material::std(0x41536A).rough(0.3);
    let dark = Material::std(0x15181D).rough(0.8);
    let parts: Vec<(geom::Mesh, Material, Quat)> = vec![
        (
            geom::cuboid(1.6, 0.14, 0.5).translate(Vec3::new(0.0, EYE - 0.44, -0.78)),
            dark.clone(),
            Quat::IDENTITY,
        ),
        (
            geom::cuboid(1.9, 0.06, 1.7).translate(Vec3::new(0.0, EYE - 0.52, -1.85)),
            body.clone(),
            Quat::IDENTITY,
        ),
        (
            geom::cuboid(1.6, 0.05, 0.8).translate(Vec3::new(0.0, EYE + 0.36, -0.3)),
            dark.clone(),
            Quat::IDENTITY,
        ),
        (
            geom::cuboid(0.08, 0.55, 1.6).translate(Vec3::new(-0.82, EYE - 0.55, -0.2)),
            dark.clone(),
            Quat::IDENTITY,
        ),
        (
            geom::cuboid(0.08, 0.55, 1.6).translate(Vec3::new(0.82, EYE - 0.55, -0.2)),
            dark.clone(),
            Quat::IDENTITY,
        ),
    ];
    for (m, mat, _) in parts {
        mesh_node(gl, scene, track, &m, mat);
    }
    for x in [-0.74f32, 0.74] {
        let p = mesh_node(
            gl,
            scene,
            track,
            &geom::cuboid(0.07, 0.85, 0.07),
            dark.clone(),
        );
        scene.set_pos(p, Vec3::new(x, EYE - 0.02, -0.72));
        scene.node(p).rotation = Quat::from_rotation_x(-0.55);
    }
    let wheel = mesh_node(
        gl,
        scene,
        track,
        &geom::torus(0.18, 0.02, 10, 40, std::f32::consts::TAU),
        dark,
    );
    scene.set_pos(wheel, Vec3::new(0.0, EYE - 0.33, -0.48));
    scene.node(wheel).rotation = Quat::from_rotation_x(-0.35);
}

// ── getting in ──

/// Runs inside the visitor's click: the XR request has to, so the browser counts the gesture.
pub fn start(app: Weak<RefCell<App>>, value: &str) {
    let Some(rc) = app.upgrade() else { return };
    let Ok(a) = rc.try_borrow() else { return };
    if a.garage_busy() {
        return;
    }
    let (gl, vr) = (a.gl_context(), a.vr_supported());
    drop(a);
    // the captured home's file comes and goes without a session
    match value {
        "download" => {
            let app = app.clone();
            spawn_local(async move {
                let said = super::homecap::download().await;
                if let Some(a) = app.upgrade() {
                    a.borrow_mut()
                        .garage_error(&said.unwrap_or_else(|e| format!("No download: {e}")));
                }
            });
            return;
        }
        "open" => {
            let app = app.clone();
            super::homecap::pick(move |result| {
                let Some(a) = app.upgrade() else { return };
                match result {
                    Ok(_) => {
                        super::homecap::load();
                        a.borrow_mut().garage_begin(Mode::Home, None, None, None);
                    }
                    Err(e) => a
                        .borrow_mut()
                        .garage_error(&format!("That file didn't open as a captured home: {e}")),
                }
            });
            return;
        }
        _ => {}
    }
    let Some(mode) = Mode::parse(value) else {
        return;
    };
    let car = !matches!(mode, Mode::Capture | Mode::Home);
    let audio = car.then(|| Audio::new().ok()).flatten();
    let Some(w) = web_sys::window() else { return };
    let xr = w.navigator().xr();
    if mode == Mode::Home {
        super::homecap::load();
    }
    let flat = match mode {
        Mode::Launch => !present(&xr) || !vr,
        Mode::Home => value != "home-vr" || !present(&xr) || !vr,
        _ => false,
    };
    if flat {
        // no headset, or none asked for: on this screen
        let app = app.clone();
        spawn_local(async move {
            if let Some(a) = app.upgrade() {
                a.borrow_mut().garage_begin(mode, None, audio, None);
            }
        });
        return;
    }
    if !present(&xr) {
        let app = app.clone();
        spawn_local(async move {
            if let Some(a) = app.upgrade() {
                a.borrow_mut().garage_error(if mode == Mode::Capture {
                    "Capturing a home needs a headset's mixed reality, and this browser has no WebXR."
                } else {
                    "Life-size and the miniature need a headset's mixed reality, and this browser has no WebXR."
                });
            }
        });
        return;
    }
    // the camera and the face finder are asked for inside this same press
    let mut cap = (mode == Mode::Capture).then(Capturing::begin);
    let init = XrSessionInit::new();
    let features: Vec<JsValue> = if mode == Mode::Capture {
        [
            "local-floor",
            "plane-detection",
            "mesh-detection",
            "hand-tracking",
        ]
        .iter()
        .map(|f| JsValue::from_str(f))
        .collect()
    } else if mode.ar() {
        [
            "local-floor",
            "hit-test",
            "plane-detection",
            "hand-tracking",
        ]
        .iter()
        .map(|f| JsValue::from_str(f))
        .collect()
    } else {
        ["local-floor", "hand-tracking"]
            .iter()
            .map(|f| JsValue::from_str(f))
            .collect()
    };
    init.set_optional_features(&features);
    let requested = xr.request_session_with_options(
        if mode.ar() {
            XrSessionMode::ImmersiveAr
        } else {
            XrSessionMode::ImmersiveVr
        },
        &init,
    );
    let requested: js_sys::Promise = requested.unchecked_into();
    rc.borrow_mut().garage_pending(true);
    spawn_local(async move {
        let result = setup(requested, gl).await;
        let Some(a) = app.upgrade() else { return };
        match result {
            Err(e) => {
                if let Some(audio) = audio {
                    audio.close();
                }
                // no session: the camera that was asked for is closed again
                drop(cap.take().map(|c| c.finish()));
                a.borrow_mut().garage_error(&format!(
                    "The headset didn't open {}: {e}",
                    if mode.ar() { "mixed reality" } else { "VR" }
                ));
            }
            Ok(state) => {
                let session = state.session.clone();
                a.borrow_mut()
                    .garage_begin(mode, Some(state), audio, cap.take());
                run(app.clone(), session);
            }
        }
    });
}

async fn setup(requested: js_sys::Promise, gl: WebGl2RenderingContext) -> Result<XrState, String> {
    let session: XrSession = JsFuture::from(requested)
        .await
        .map_err(|e| js_error(&e))?
        .unchecked_into();
    let ready = async {
        JsFuture::from(gl.make_xr_compatible())
            .await
            .map_err(|e| js_error(&e))?;
        let layer = XrWebGlLayer::new_with_web_gl2_rendering_context(&session, &gl)
            .map_err(|e| js_error(&e))?;
        let state = XrRenderStateInit::new();
        state.set_base_layer(Some(&layer));
        state.set_depth_near(0.02);
        state.set_depth_far(1500.0);
        session.update_render_state_with_state(&state);
        let (space, floor) =
            match JsFuture::from(session.request_reference_space(XrReferenceSpaceType::LocalFloor))
                .await
            {
                Ok(s) => (s, true),
                Err(_) => (
                    JsFuture::from(session.request_reference_space(XrReferenceSpaceType::Local))
                        .await
                        .map_err(|e| js_error(&e))?,
                    false,
                ),
            };
        Ok::<_, String>((layer, space.unchecked_into::<XrReferenceSpace>(), floor))
    }
    .await;
    match ready {
        Ok((layer, space, floor)) => Ok(XrState {
            session,
            layer,
            space,
            floor,
            hit: Rc::new(RefCell::new(HashMap::new())),
            asked: HashSet::new(),
        }),
        Err(e) => {
            let _ = session.end();
            Err(e)
        }
    }
}

type Loop = Rc<RefCell<Option<Closure<dyn FnMut(f64, XrFrame)>>>>;

fn run(app: Weak<RefCell<App>>, session: XrSession) {
    let frames: Loop = Rc::new(RefCell::new(None));
    let ended = frames.clone();
    let weak = app.clone();
    let end = Closure::<dyn FnMut()>::new(move || {
        ended.borrow_mut().take();
        let weak = weak.clone();
        spawn_local(async move {
            if let Some(a) = weak.upgrade() {
                a.borrow_mut().garage_ended();
            }
        });
    });
    session.set_onend(Some(end.as_ref().unchecked_ref()));
    end.forget();
    let weak = app.clone();
    let select = Closure::<dyn FnMut(XrInputSourceEvent)>::new(move |e: XrInputSourceEvent| {
        if let Some(a) = weak.upgrade() {
            if let Ok(mut a) = a.try_borrow_mut() {
                a.garage_select(&e);
            }
        }
    });
    session.set_onselectstart(Some(select.as_ref().unchecked_ref()));
    select.forget();
    let next = frames.clone();
    let current = session.clone();
    *frames.borrow_mut() = Some(Closure::new(move |now: f64, frame: XrFrame| {
        if let Some(a) = app.upgrade() {
            a.borrow_mut().garage_xr_frame(now, &frame);
        }
        if let Some(cb) = next.borrow().as_ref() {
            current.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    if let Some(cb) = frames.borrow().as_ref() {
        session.request_animation_frame(cb.as_ref().unchecked_ref());
    };
}
