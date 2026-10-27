//! Lucy, the axolotl: my form in the room, from his hand-off of 2026-10-25. Freckled skin, a
//! wide flat head, six luminous gill fans that carry the verdict colour, a dorsal fin and a
//! caudal fin that wags with the tail, flat feet with toes. The gills glow while the engine
//! thinks, for exactly as long as it thinks, and flare when it answers.

use glam::{Mat4, Vec3};
use web_sys::HtmlCanvasElement;

use crate::colors::{self as col, linear};
use crate::geom;
use crate::gl::Gl;
use crate::scene::{euler, Kind, Material, Node, NodeId, Scene};
use crate::util::{angle_delta, clamp, ease, Rng};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Idle,
    Walk,
    Swim,
    Return,
}

pub struct State {
    pub verdict: [f32; 3],
    pub flare: f32,
    pub thinking: bool,
    pub mode: Mode,
    pub home: Vec3,
    pub target: Vec3,
    pub blink_at: f32,
    pub blink_until: f32,
    pub swim_a: f32,
    pub swim_h: f32,
    pub idle_since: f32,
    pub wag: f32,
    pub scale: f32,
    pub awake: f32,
    pub flutter: f32,
    pub flutter_at: f32,
    pub tilt: f32,
    pub look_at: f32,
    pub look_x: f32,
    pub look_y: f32,
    pub glance_until: f32,
    pub rng: Rng,
}

struct Gill {
    node: NodeId,
    base_x: f32,
    base_z: f32,
    i: usize,
}
struct Leg {
    node: NodeId,
    base_x: f32,
    phase: f32,
}

pub struct Lucy {
    pub group: NodeId,
    pub head: NodeId,
    body: NodeId,
    eyes: [NodeId; 2],
    gills: Vec<Gill>,
    gill_meshes: Vec<NodeId>,
    tips: Vec<NodeId>,
    legs: Vec<Leg>,
    tail: Vec<(NodeId, f32)>,
    caudal: NodeId,
    glow: NodeId,
    light: NodeId,
    shadow: NodeId,
    pub st: State,
}

fn canvas(w: u32, h: u32) -> (HtmlCanvasElement, web_sys::CanvasRenderingContext2d) {
    use wasm_bindgen::JsCast;
    let doc = web_sys::window().unwrap().document().unwrap();
    let c: HtmlCanvasElement = doc.create_element("canvas").unwrap().dyn_into().unwrap();
    c.set_width(w);
    c.set_height(h);
    let ctx: web_sys::CanvasRenderingContext2d =
        c.get_context("2d").unwrap().unwrap().dyn_into().unwrap();
    (c, ctx)
}
/// A soft radial halo, for the gill tips and the glow behind her head.
pub fn halo_texture(gl: &Gl) -> crate::gl::Texture {
    let (c, g) = canvas(128, 128);
    let grad = g
        .create_radial_gradient(64.0, 64.0, 2.0, 64.0, 64.0, 64.0)
        .unwrap();
    grad.add_color_stop(0.0, "rgba(255,255,255,1)").unwrap();
    grad.add_color_stop(0.35, "rgba(255,255,255,0.35)").unwrap();
    grad.add_color_stop(1.0, "rgba(255,255,255,0)").unwrap();
    g.set_fill_style_canvas_gradient(&grad);
    g.fill_rect(0.0, 0.0, 128.0, 128.0);
    gl.texture_from_canvas(&c, true)
}
/// Freckled skin: soft pink, darker speckles, a few pale flecks.
fn skin_texture(gl: &Gl, rng: &mut Rng) -> crate::gl::Texture {
    let (c, g) = canvas(256, 256);
    g.set_fill_style_str("#F7DDD7");
    g.fill_rect(0.0, 0.0, 256.0, 256.0);
    for _ in 0..1100 {
        let (r, gg, b, a) = (
            150 + (rng.f32() * 40.0) as u32,
            88 + (rng.f32() * 30.0) as u32,
            94 + (rng.f32() * 30.0) as u32,
            0.07 + rng.f32() * 0.15,
        );
        g.set_fill_style_str(&format!("rgba({r},{gg},{b},{a})"));
        g.begin_path();
        let _ = g.arc(
            rng.f32() as f64 * 256.0,
            rng.f32() as f64 * 256.0,
            (0.6 + rng.f32() * 2.0) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        g.fill();
    }
    for _ in 0..180 {
        g.set_fill_style_str(&format!("rgba(255,250,246,{})", 0.08 + rng.f32() * 0.12));
        g.begin_path();
        let _ = g.arc(
            rng.f32() as f64 * 256.0,
            rng.f32() as f64 * 256.0,
            (1.0 + rng.f32() * 2.5) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        g.fill();
    }
    gl.texture_from_canvas(&c, false)
}
fn basic(hex: u32) -> Material {
    let mut m = Material::std(hex);
    m.color = [0.0, 0.0, 0.0];
    m.emissive = linear(hex);
    m.emissive_intensity = 1.0;
    m
}

const SKULL_RADIUS: f32 = 0.115;
const SKULL_SCALE: Vec3 = Vec3::new(1.26, 0.78, 1.04);

/// Embed a shallow smile in the head's ellipsoid instead of floating a flat arc in front.
fn smile_mesh() -> geom::Mesh {
    let mut mesh = geom::torus(0.05, 0.0035, 8, 28, std::f32::consts::PI);
    let radii = SKULL_SCALE * SKULL_RADIUS;
    for (p, n) in mesh.positions.chunks_mut(3).zip(mesh.normals.chunks_mut(3)) {
        let x = -p[0];
        let y = -0.024 - 0.36 * p[1];
        let z = radii.z * (1.0 - (x / radii.x).powi(2) - (y / radii.y).powi(2)).sqrt();
        let dz_dx = -radii.z.powi(2) * x / (radii.x.powi(2) * z);
        let dz_dy = -radii.z.powi(2) * y / (radii.y.powi(2) * z);
        let normal = Vec3::new(-n[0] - dz_dx * n[2], -n[1] / 0.36 - dz_dy * n[2], n[2]).normalize();
        // The tube crosses the surface along its whole length, including between facets.
        p.copy_from_slice(&[x, y, z + p[2]]);
        n.copy_from_slice(&normal.to_array());
    }
    mesh
}

impl Lucy {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId, seed: u64) -> Lucy {
        let mut rng = Rng::new(seed);
        let skin_tex = scene.add_texture(skin_texture(gl, &mut rng));
        let halo = scene.add_texture(halo_texture(gl));
        let skin = {
            let mut m = Material::std(0xFFFFFF)
                .rough(0.55)
                .emissive(0x6A4642)
                .glow(0.22);
            m.map = Some(skin_tex);
            m
        };
        let fin_mat = Material::std(0xF2D7D0)
            .emissive(0x5A3330)
            .glow(0.12)
            .two_sided()
            .alpha(0.55)
            .rough(0.7)
            .no_depth_write();
        let mesh = |scene: &mut Scene, m: &geom::Mesh| scene.add_mesh(gl.upload(m));
        let g = scene.add(parent, Node::new(Kind::Empty));
        // body
        let body_mesh = mesh(scene, &geom::capsule(0.088, 0.26, 8, 20));
        let body = scene.add(
            g,
            Node::new(Kind::Mesh {
                mesh: body_mesh,
                material: skin.clone(),
            }),
        );
        scene.set_rot(body, std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        scene.set_pos(body, Vec3::new(0.0, 0.105, 0.0));
        // head
        let head = scene.add(g, Node::new(Kind::Empty));
        scene.set_pos(head, Vec3::new(0.0, 0.125, 0.19));
        let skull_mesh = mesh(scene, &geom::sphere(SKULL_RADIUS, 32, 22));
        let skull = scene.add(
            head,
            Node::new(Kind::Mesh {
                mesh: skull_mesh,
                material: skin.clone(),
            }),
        );
        scene.set_scale(skull, SKULL_SCALE);
        let eye_mesh = mesh(scene, &geom::sphere(0.02, 16, 12));
        let hi_mesh = mesh(scene, &geom::sphere(0.006, 8, 6));
        let mut eyes = [0; 2];
        for (k, s) in [-1.0f32, 1.0].iter().enumerate() {
            let e = scene.add(head, Node::new(Kind::Empty));
            scene.set_pos(e, Vec3::new(s * 0.086, 0.034, 0.088));
            scene.add(
                e,
                Node::new(Kind::Mesh {
                    mesh: eye_mesh,
                    material: Material::std(col::VOID_900).rough(0.25),
                }),
            );
            let hi = scene.add(
                e,
                Node::new(Kind::Mesh {
                    mesh: hi_mesh,
                    material: basic(col::PAPER_100),
                }),
            );
            scene.set_pos(hi, Vec3::new(s * 0.005, 0.008, 0.015));
            eyes[k] = e;
        }
        let smile_mesh = mesh(scene, &smile_mesh());
        scene.add(
            head,
            Node::new(Kind::Mesh {
                mesh: smile_mesh,
                material: basic(0x6B4440),
            }),
        );
        // gills: three stalks a side, six filaments each, a lit tip
        let gill_mat = Material::std(col::PHOSPHOR_500).rough(0.5).glow(1.1);
        let mut gills = Vec::new();
        let mut gill_meshes = Vec::new();
        let mut tips = Vec::new();
        for s in [-1.0f32, 1.0] {
            for (i, k) in [-1.0f32, 0.0, 1.0].iter().enumerate() {
                let gl_node = scene.add(head, Node::new(Kind::Empty));
                scene.set_pos(gl_node, Vec3::new(s * 0.1, 0.05, -0.005 - i as f32 * 0.036));
                let len = 0.15 - i as f32 * 0.014;
                let stalk_mesh = mesh(scene, &geom::cylinder(0.0035, 0.011, len, 8));
                let stalk = scene.add(
                    gl_node,
                    Node::new(Kind::Mesh {
                        mesh: stalk_mesh,
                        material: gill_mat.clone(),
                    }),
                );
                scene.set_pos(stalk, Vec3::new(0.0, len / 2.0, 0.0));
                gill_meshes.push(stalk);
                for j in 0..6 {
                    let fl = 0.06 - j as f32 * 0.0065;
                    let fm = mesh(
                        scene,
                        &geom::cylinder(0.0016, 0.0042, fl, 6).translate(Vec3::new(
                            0.0,
                            fl / 2.0,
                            0.0,
                        )),
                    );
                    let f = scene.add(
                        gl_node,
                        Node::new(Kind::Mesh {
                            mesh: fm,
                            material: gill_mat.clone(),
                        }),
                    );
                    scene.set_pos(
                        f,
                        Vec3::new(0.0, 0.03 + j as f32 * (len - 0.045) / 5.0, 0.0),
                    );
                    let sign = if j % 2 == 1 { -1.0 } else { 1.0 };
                    scene.set_rot(f, sign * 0.45 * s, 0.0, -sign * 1.1);
                    gill_meshes.push(f);
                }
                let tip = scene.add(
                    gl_node,
                    Node::new(Kind::Sprite {
                        texture: halo,
                        color: linear(col::PHOSPHOR_500),
                        opacity: 0.5,
                        additive: true,
                    }),
                );
                scene.set_scale(tip, Vec3::splat(0.085));
                scene.set_pos(tip, Vec3::new(0.0, len, 0.0));
                tips.push(tip);
                let (bx, bz) = (-0.35 + k * 0.35, -s);
                scene.set_rot(gl_node, bx, 0.0, bz);
                gills.push(Gill {
                    node: gl_node,
                    base_x: bx,
                    base_z: bz,
                    i: gills.len(),
                });
            }
        }
        // legs: hip, a segment, a flat foot with three toes
        let seg_mesh = mesh(scene, &geom::capsule(0.016, 0.07, 4, 8));
        let pad_mesh = mesh(scene, &geom::sphere(0.017, 10, 8));
        let toe_mesh = mesh(scene, &geom::capsule(0.005, 0.016, 3, 6));
        let mut legs = Vec::new();
        for (i, (sx, sz)) in [(-1.0f32, 1.0f32), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .iter()
            .enumerate()
        {
            let hip = scene.add(g, Node::new(Kind::Empty));
            scene.set_pos(hip, Vec3::new(sx * 0.072, 0.075, sz * 0.1));
            let seg = scene.add(
                hip,
                Node::new(Kind::Mesh {
                    mesh: seg_mesh,
                    material: skin.clone(),
                }),
            );
            scene.set_pos(seg, Vec3::new(0.0, -0.045, 0.0));
            let foot = scene.add(hip, Node::new(Kind::Empty));
            scene.set_pos(foot, Vec3::new(0.0, -0.09, 0.012));
            scene.set_rot(foot, -sz * 0.25, 0.0, -sx * 0.55);
            let pad = scene.add(
                foot,
                Node::new(Kind::Mesh {
                    mesh: pad_mesh,
                    material: skin.clone(),
                }),
            );
            scene.set_scale(pad, Vec3::new(1.1, 0.5, 1.3));
            for tz in [-1.0f32, 0.0, 1.0] {
                let toe = scene.add(
                    foot,
                    Node::new(Kind::Mesh {
                        mesh: toe_mesh,
                        material: skin.clone(),
                    }),
                );
                scene.set_rot(toe, std::f32::consts::FRAC_PI_2 - 0.15, tz * 0.45, 0.0);
                scene.set_pos(toe, Vec3::new(tz * 0.01, -0.003, 0.022));
            }
            scene.set_rot(hip, sz * 0.25, 0.0, sx * 0.55);
            legs.push(Leg {
                node: hip,
                base_x: sz * 0.25,
                phase: (i % 2) as f32 * std::f32::consts::PI,
            });
        }
        // tail and fins
        let mut tail = Vec::new();
        let mut z = -0.17;
        for (i, r) in [0.07f32, 0.056, 0.044, 0.032, 0.022, 0.013]
            .iter()
            .enumerate()
        {
            let tm = mesh(scene, &geom::sphere(*r, 16, 12));
            let n = scene.add(
                g,
                Node::new(Kind::Mesh {
                    mesh: tm,
                    material: skin.clone(),
                }),
            );
            scene.set_scale(n, Vec3::new(0.85, 1.25, 1.0));
            let y0 = 0.095 + i as f32 * 0.004;
            scene.set_pos(n, Vec3::new(0.0, y0, z));
            z -= r * 1.25;
            tail.push((n, y0));
        }
        let dorsal_mesh = mesh(
            scene,
            &geom::fin(
                &[
                    (-0.08, 0.1),
                    (-0.02, 0.21),
                    (0.08, 0.255),
                    (0.19, 0.265),
                    (0.3, 0.24),
                ],
                &[(0.3, 0.09), (0.1, 0.085)],
            ),
        );
        scene.add(
            g,
            Node::new(Kind::Mesh {
                mesh: dorsal_mesh,
                material: fin_mat.clone(),
            }),
        );
        let caudal = scene.add(g, Node::new(Kind::Empty));
        scene.set_pos(caudal, Vec3::new(0.0, 0.0, -0.3));
        let caudal_mesh = mesh(
            scene,
            &geom::fin(
                &[
                    (0.0, 0.09),
                    (0.035, 0.235),
                    (0.11, 0.215),
                    (0.17, 0.16),
                    (0.22, 0.1),
                    (0.17, 0.04),
                    (0.1, 0.035),
                    (0.035, 0.065),
                ],
                &[],
            ),
        );
        scene.add(
            caudal,
            Node::new(Kind::Mesh {
                mesh: caudal_mesh,
                material: fin_mat,
            }),
        );
        // halo, light, shadow
        let glow = scene.add(
            g,
            Node::new(Kind::Sprite {
                texture: halo,
                color: linear(col::PHOSPHOR_500),
                opacity: 0.3,
                additive: true,
            }),
        );
        scene.set_scale(glow, Vec3::splat(0.9));
        scene.set_pos(glow, Vec3::new(0.0, 0.18, -0.06));
        let light = scene.add(
            g,
            Node::new(Kind::Light {
                color: linear(col::PHOSPHOR_500),
                intensity: 0.35,
                distance: 3.4,
                decay: 1.6,
            }),
        );
        scene.set_pos(light, Vec3::new(0.0, 0.85, -0.05));
        let shadow_mesh = mesh(scene, &geom::circle(0.3, 28));
        let shadow = scene.add(
            g,
            Node::new(Kind::Mesh {
                mesh: shadow_mesh,
                material: basic(0x000000).alpha(0.5).no_depth_write(),
            }),
        );
        scene.set_rot(shadow, -std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        scene.set_pos(shadow, Vec3::new(0.0, 0.003, 0.0));
        let st = State {
            verdict: linear(col::PHOSPHOR_500),
            flare: 0.0,
            thinking: false,
            mode: Mode::Idle,
            home: Vec3::ZERO,
            target: Vec3::ZERO,
            blink_at: 2.5,
            blink_until: 0.0,
            swim_a: 0.0,
            swim_h: 0.0,
            idle_since: 0.0,
            wag: 1.0,
            scale: 1.0,
            awake: 1.0,
            flutter: 0.0,
            flutter_at: 6.0,
            tilt: 0.0,
            look_at: 4.0,
            look_x: 0.0,
            look_y: 0.0,
            glance_until: 0.0,
            rng,
        };
        Lucy {
            group: g,
            head,
            body,
            eyes,
            gills,
            gill_meshes,
            tips,
            legs,
            tail,
            caudal,
            glow,
            light,
            shadow,
            st,
        }
    }
    /// The verdict colour: gills, tips, halo and light.
    pub fn set_color(&mut self, scene: &mut Scene, hex: u32) {
        let c = linear(hex);
        self.st.verdict = c;
        for &n in &self.gill_meshes {
            if let Some(m) = scene.material(n) {
                m.color = c;
                m.emissive = c;
            }
        }
        for &t in &self.tips {
            if let Kind::Sprite { color, .. } = &mut scene.node(t).kind {
                *color = c;
            }
        }
        if let Kind::Sprite { color, .. } = &mut scene.node(self.glow).kind {
            *color = c;
        }
        if let Kind::Light { color, .. } = &mut scene.node(self.light).kind {
            *color = c;
        }
    }
    pub fn flare(&mut self) {
        self.st.flare = 1.0;
    }
    pub fn set_thinking(&mut self, v: bool) {
        self.st.thinking = v;
    }
    pub fn poke(&mut self) {
        self.st.idle_since = 0.0;
        if self.st.mode == Mode::Swim {
            self.st.mode = Mode::Return;
        }
    }
    pub fn set_scale(&mut self, scene: &mut Scene, s: f32) {
        self.st.scale = s;
        scene.set_scale(self.group, Vec3::splat(s));
    }
    /// Walk a few steps toward the visitor, head cocked, gills astir.
    pub fn call(&mut self, scene: &Scene, cam_pos: Vec3) {
        let st = &mut self.st;
        st.tilt = if st.rng.chance(0.5) { -0.26 } else { 0.26 };
        st.flutter = st.flutter.max(0.7);
        let pos = scene.nodes[self.group].position;
        let mut d = cam_pos - pos;
        d.y = 0.0;
        let dist = d.length();
        if dist < 1.1 {
            st.mode = Mode::Idle;
            return;
        }
        let step = d.normalize() * (0.45f32).min(dist - 1.0);
        st.target = pos + step;
        let mut off = st.target - st.home;
        off.y = 0.0;
        if off.length() > 0.5 {
            off = off.normalize() * 0.5;
            st.target = st.home + off;
        }
        st.target.y = st.home.y;
        st.mode = Mode::Walk;
        st.idle_since = 0.0;
    }
    /// 0 dark → 1 awake: she scales in, then the gills light one by one.
    pub fn wake(&mut self, scene: &mut Scene, p: f32) {
        self.st.awake = p;
        scene.set_visible(self.group, p > 0.3);
        let s = self.st.scale * (0.5 + 0.5 * ease((p - 0.3) / 0.25));
        scene.set_scale(self.group, Vec3::splat(s));
        for (i, g) in self.gills.iter().enumerate() {
            let q = ease((p - (0.55 + i as f32 * 0.06)) / 0.08);
            scene.set_visible(g.node, q > 0.0);
            scene.set_scale(g.node, Vec3::splat(q.max(0.01)));
        }
        if let Kind::Light { intensity, .. } = &mut scene.node(self.light).kind {
            *intensity = 0.35 * ease((p - 0.5) / 0.45);
        }
        if let Kind::Sprite { opacity, .. } = &mut scene.node(self.glow).kind {
            *opacity = 0.3 * ease((p - 0.5) / 0.45);
        }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32, dt: f32, cam_pos: Vec3) {
        if !scene.nodes[self.group].visible {
            return;
        }
        let st = &mut self.st;
        st.flare = (st.flare - dt * 0.8).max(0.0);
        st.idle_since += dt;
        if t > st.flutter_at {
            st.flutter = 1.0;
            st.flutter_at = t + 7.0 + st.rng.f32() * 9.0;
        }
        st.flutter = (st.flutter - dt * 1.4).max(0.0);
        st.tilt += (0.0 - st.tilt) * dt * 1.4;
        // the thinking pulse is on only while the engine runs a query: exactly the engine's time
        let think = if st.thinking {
            0.7 * (t * 7.0).sin().abs() + 0.3
        } else {
            0.0
        };
        let lit = ease((st.awake - 0.5) / 0.45);
        let breath = 1.0 + 0.03 * (t * 1.6).sin();
        scene.set_scale(self.body, Vec3::new(breath, 1.0, breath));
        let ripple = st.flutter * (t * 22.0).sin() * 0.16;
        let fan = st.flare * 0.14 + st.flutter * 0.05 + think * 0.1;
        for g in &self.gills {
            let sign = if g.i % 2 == 1 { 1.0 } else { -1.0 };
            let rx = g.base_x + (t * 1.3 + g.i as f32 * 0.9).sin() * 0.1 + ripple * sign;
            let rz = g.base_z + (t * 0.9 + g.i as f32).sin() * 0.05 + g.base_z.signum() * fan;
            scene.set_rot(g.node, rx, 0.0, rz);
        }
        let gill_i = (0.5 + 0.15 * (t * 1.6).sin() + st.flare * 1.6 + think)
            * if st.awake > 0.95 { 1.0 } else { 0.8 };
        for &n in &self.gill_meshes {
            if let Some(m) = scene.material(n) {
                m.emissive_intensity = gill_i;
            }
        }
        let tip_o = (0.16 + 0.08 * (t * 1.6 + 1.0).sin() + st.flare * 0.5 + think * 0.3) * lit;
        let ts = 0.085 + st.flare * 0.05 + think * 0.03;
        for &tp in &self.tips {
            if let Kind::Sprite { opacity, .. } = &mut scene.node(tp).kind {
                *opacity = tip_o;
            }
            scene.set_scale(tp, Vec3::splat(ts));
        }
        if let Kind::Sprite { opacity, .. } = &mut scene.node(self.glow).kind {
            *opacity = (0.08 + 0.04 * (t * 1.6).sin() + st.flare * 0.35 + think * 0.25) * lit;
        }
        scene.set_scale(self.glow, Vec3::splat(0.9 + st.flare * 0.5));
        if let Kind::Light { intensity, .. } = &mut scene.node(self.light).kind {
            *intensity = (0.35 + st.flare * 2.0 + think * 1.0) * lit;
        }
        // blink
        if t > st.blink_at {
            st.blink_until = t + 0.12;
            st.blink_at = t + 2.4 + st.rng.f32() * 3.6;
            if st.rng.chance(0.25) {
                st.blink_at = t + 0.35;
            }
        }
        let open = if t < st.blink_until { 0.12 } else { 1.0 };
        for &e in &self.eyes {
            let n = scene.node(e);
            n.scale.y += (open - n.scale.y) * 0.6;
        }
        // look at the visitor; glance around when alone, and now and then
        let inv = scene.nodes[self.group].world.inverse();
        let local = inv.transform_point3(cam_pos) - scene.nodes[self.head].position;
        let near = local.length() < 4.5 / st.scale;
        if t > st.look_at {
            st.look_at = t + 3.0 + st.rng.f32() * 5.0;
            st.look_x = (st.rng.f32() - 0.5) * 1.1;
            st.look_y = (st.rng.f32() - 0.5) * 0.3;
            if near && st.rng.chance(0.35) {
                st.glance_until = t + 1.2 + st.rng.f32() * 0.8;
            }
        }
        let at_visitor = near && t > st.glance_until;
        let yaw = if at_visitor {
            clamp(local.x.atan2(local.z), -0.7, 0.7)
        } else {
            st.look_x
        };
        let pitch = if at_visitor {
            clamp(local.y.atan2(local.x.hypot(local.z)), -0.25, 0.5)
        } else {
            -st.look_y
        };
        {
            let (hx, hy, hz) = head_euler(scene, self.head);
            let nx = hx + (-pitch - hx) * 0.06;
            let ny = hy + (yaw - hy) * 0.06;
            let nz = hz + (st.tilt - hz) * 0.08;
            scene.set_rot(self.head, nx, ny, nz);
            set_head_euler(scene, self.head, (nx, ny, nz));
        }
        // locomotion
        let mut leg_swing = 0.0;
        let mut bob = 0.0;
        let pos = scene.nodes[self.group].position;
        let mut rot_y = group_yaw(scene, self.group);
        let mut new_pos = pos;
        match st.mode {
            Mode::Walk => {
                let mut d = st.target - pos;
                d.y = 0.0;
                let dist = d.length();
                if dist < 0.02 {
                    st.mode = Mode::Idle;
                } else {
                    let step = dist.min(0.45 * dt);
                    new_pos += d.normalize() * step;
                    let want = d.x.atan2(d.z);
                    rot_y += angle_delta(rot_y, want) * 0.12;
                    leg_swing = 0.6;
                    bob = 0.012 * (t * 9.0).sin().abs();
                    st.wag = 1.6;
                }
            }
            Mode::Swim => {
                st.swim_a += dt * 0.55;
                st.swim_h += (0.75 - st.swim_h) * dt * 1.2;
                let r = 0.8;
                let goal = Vec3::new(
                    st.home.x + st.swim_a.sin() * r,
                    st.home.y + st.swim_h,
                    st.home.z + st.swim_a.cos() * r - r,
                );
                new_pos = pos.lerp(goal, 0.08);
                rot_y += angle_delta(rot_y, st.swim_a + std::f32::consts::FRAC_PI_2) * 0.08;
                st.wag = 2.6;
                if st.swim_a > std::f32::consts::TAU {
                    st.mode = Mode::Return;
                }
            }
            Mode::Return => {
                new_pos = pos.lerp(st.home, (dt * 1.4).min(1.0));
                st.swim_h = new_pos.y - st.home.y;
                st.wag = 1.8;
                if new_pos.distance(st.home) < 0.03 {
                    new_pos = st.home;
                    st.mode = Mode::Idle;
                    st.swim_a = 0.0;
                    st.swim_h = 0.0;
                }
            }
            Mode::Idle => {
                st.wag += (1.0 - st.wag) * dt * 2.0;
                let to_cam = cam_pos - pos;
                let want = to_cam.x.atan2(to_cam.z);
                if to_cam.length() < 5.0 {
                    rot_y += angle_delta(rot_y, want) * 0.02;
                }
                if st.idle_since > 28.0 && st.awake > 0.95 {
                    st.mode = Mode::Swim;
                    st.swim_a = 0.0;
                    st.idle_since = 0.0;
                }
            }
        }
        let tuck = if st.mode == Mode::Swim { 1.1 } else { 0.0 };
        for l in &self.legs {
            let cur = leg_x(scene, l.node);
            let want = l.base_x + tuck + (t * 9.0 + l.phase).sin() * leg_swing;
            let nx = cur + (want - cur) * 0.3;
            let (_, _, z) = leg_euler(scene, l.node);
            scene.set_rot(l.node, nx, 0.0, z);
            set_leg_euler(scene, l.node, (nx, 0.0, z));
        }
        if st.mode != Mode::Swim && st.mode != Mode::Return {
            new_pos.y = st.home.y + bob;
        }
        scene.set_pos(self.group, new_pos);
        scene.nodes[self.group].rotation = euler(0.0, rot_y, 0.0);
        set_group_yaw(scene, self.group, rot_y);
        let wag = st.wag;
        for (i, (n, y0)) in self.tail.iter().enumerate() {
            let node = scene.node(*n);
            node.position.x = (t * 2.1 - i as f32 * 0.55).sin() * 0.011 * (i as f32 + 1.0) * wag;
            node.position.y = y0
                + if i >= 3 {
                    (t * 1.1 + 0.6).sin() * 0.003 * (i as f32 - 2.0)
                } else {
                    0.0
                };
        }
        let tail2_x = scene.nodes[self.tail[2].0].position.x;
        {
            let c = scene.node(self.caudal);
            c.position.x = tail2_x;
            c.rotation = euler(0.0, (t * 2.1 - 1.6).sin() * 0.22 * wag, 0.0);
        }
        let lift = new_pos.y - st.home.y;
        let gs = scene.nodes[self.group].scale.y.max(0.01);
        let sh = 1.0 / (1.0 + lift * 1.5);
        {
            let s = scene.node(self.shadow);
            s.position.y = -lift / gs + 0.003;
            s.scale = Vec3::splat(sh);
        }
        if let Some(m) = scene.material(self.shadow) {
            m.opacity = 0.5 * sh;
        }
    }
}

// Euler bookkeeping: glam quaternions do not keep the three angles, so the few nodes that are
// eased per axis remember them here, in a tiny side table on the node's scale sign bits? No:
// in a thread-local map keyed by node, honest and simple.
thread_local! { static EULERS: std::cell::RefCell<std::collections::HashMap<NodeId, (f32, f32, f32)>> = std::cell::RefCell::new(std::collections::HashMap::new()); }
fn get_euler(id: NodeId) -> (f32, f32, f32) {
    EULERS.with(|m| *m.borrow().get(&id).unwrap_or(&(0.0, 0.0, 0.0)))
}
fn put_euler(id: NodeId, e: (f32, f32, f32)) {
    EULERS.with(|m| {
        m.borrow_mut().insert(id, e);
    });
}
fn head_euler(_s: &Scene, id: NodeId) -> (f32, f32, f32) {
    get_euler(id)
}
fn set_head_euler(_s: &mut Scene, id: NodeId, e: (f32, f32, f32)) {
    put_euler(id, e)
}
fn leg_euler(_s: &Scene, id: NodeId) -> (f32, f32, f32) {
    get_euler(id)
}
fn leg_x(s: &Scene, id: NodeId) -> f32 {
    leg_euler(s, id).0
}
fn set_leg_euler(_s: &mut Scene, id: NodeId, e: (f32, f32, f32)) {
    put_euler(id, e)
}
fn group_yaw(_s: &Scene, id: NodeId) -> f32 {
    get_euler(id).1
}
fn set_group_yaw(_s: &mut Scene, id: NodeId, y: f32) {
    put_euler(id, (0.0, y, 0.0))
}
/// The legs start at their built pose; call once after `build` so the eased angles begin there.
pub fn seed_eulers(l: &Lucy) {
    for leg in &l.legs {
        put_euler(leg.node, (leg.base_x, 0.0, 0.0));
    }
    put_euler(l.head, (0.0, 0.0, 0.0));
    put_euler(l.group, (0.0, 0.0, 0.0));
}
#[allow(dead_code)]
fn _unused(_: Mat4) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_smile_section_intersects_the_rendered_skull() {
        let skull = geom::sphere(SKULL_RADIUS, 32, 22);
        let surface_z = |p: Vec3| {
            skull
                .indices
                .chunks(3)
                .filter_map(|triangle| {
                    let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| {
                        Vec3::from_slice(&skull.positions[i as usize * 3..i as usize * 3 + 3])
                            * SKULL_SCALE
                    });
                    let det = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
                    if det.abs() < 1e-10 {
                        return None;
                    }
                    let u = ((b.y - c.y) * (p.x - c.x) + (c.x - b.x) * (p.y - c.y)) / det;
                    let v = ((c.y - a.y) * (p.x - c.x) + (a.x - c.x) * (p.y - c.y)) / det;
                    let w = 1.0 - u - v;
                    (u >= -1e-5 && v >= -1e-5 && w >= -1e-5).then_some(u * a.z + v * b.z + w * c.z)
                })
                .max_by(f32::total_cmp)
                .expect("smile lies over the face")
        };
        let smile = smile_mesh();
        for section in 0..=28 {
            let distances: Vec<_> = (0..=8)
                .map(|radial| {
                    let vertex = (radial * 29 + section) * 3;
                    let p = Vec3::from_slice(&smile.positions[vertex..vertex + 3]);
                    p.z - surface_z(p)
                })
                .collect();
            assert!(
                distances.iter().any(|d| *d < 0.0),
                "detached section {section}"
            );
            assert!(
                distances.iter().any(|d| *d > 0.0),
                "buried section {section}"
            );
            assert!(
                distances.iter().all(|d| d.abs() < 0.005),
                "excessive offset at {section}"
            );
        }
    }
}
