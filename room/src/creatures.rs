//! The cast's forms (roadmap/N8d.md), ported from his hand-off's `_creatures.mjs`: the birds and
//! their perch, the dogs lying like sphinxes, his chair, the dot that is Nell, and two forms of
//! mine for the names the hand-off did not draw: a person figure (a translucent capsule and its
//! wire, as the hand-off drew a visitor) and a sign on a post. Each form animates itself.

use glam::Vec3;

use crate::colors::{self as col, linear};
use crate::geom;
use crate::gl::Gl;
use crate::scene::{euler, Kind, Material, Node, NodeId, Scene};
use crate::util::Rng;

fn std(color: u32, glow: f32, rough: f32) -> Material {
    Material::std(color).glow(glow).rough(rough)
}
fn basic(hex: u32) -> Material {
    let mut m = Material::std(hex);
    m.color = [0.0, 0.0, 0.0];
    m.emissive = linear(hex);
    m.emissive_intensity = 1.0;
    m
}
fn mesh_node(
    gl: &Gl,
    scene: &mut Scene,
    parent: NodeId,
    m: &geom::Mesh,
    material: Material,
    pos: Vec3,
    rot: (f32, f32, f32),
    scale: Vec3,
) -> NodeId {
    let mesh = scene.add_mesh(gl.upload(m));
    let n = scene.add(parent, Node::new(Kind::Mesh { mesh, material }));
    scene.set_pos(n, pos);
    scene.set_rot(n, rot.0, rot.1, rot.2);
    scene.set_scale(n, scale);
    n
}

// ── birds ──
/// A species' palette: body, head, face (mask, cheeks, the breast), wing, tail, beak, eye ring.
#[derive(Clone, Copy, Debug)]
pub struct BirdPalette {
    pub body: u32,
    pub head: u32,
    pub face: Option<u32>,
    pub wing: u32,
    pub tail: u32,
    pub beak: u32,
    pub eye_ring: Option<u32>,
}
pub const AFRICAN_GREY: BirdPalette = BirdPalette {
    body: 0x8E8E95,
    head: 0x9A9AA2,
    face: Some(0xDADAE0),
    wing: 0x5C5C66,
    tail: 0xE0352B,
    beak: 0x1A1A20,
    eye_ring: Some(0xE8E6E3),
};
pub const SUN_CONURE: BirdPalette = BirdPalette {
    body: 0xF5C542,
    head: 0xF5B544,
    face: Some(0xF26A2D),
    wing: 0x3DAE5C,
    tail: 0x4E8F3A,
    beak: 0x1A1A20,
    eye_ring: None,
};
/// A bird whose looks I do not know: the hand-off's sky, as it drew a bird merely seen.
pub const UNKNOWN_BIRD: BirdPalette = BirdPalette {
    body: col::SKY_500,
    head: col::VOID_400,
    face: None,
    wing: 0x2A7FB8,
    tail: col::SKY_500,
    beak: 0x1A1A20,
    eye_ring: None,
};

pub struct Bird {
    pub group: NodeId,
    head: NodeId,
    wings: [NodeId; 2],
    wing_z: [f32; 2],
    home_x: f32,
    hop_at: f32,
    hop_t: f32,
    x0: f32,
    dx: f32,
    y0: Option<f32>,
    look_at: f32,
    look: f32,
    head_y: f32,
    ruffle_at: f32,
    ruffle: f32,
    rng: Rng,
}
impl Bird {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId, p: BirdPalette, seed: u64) -> Bird {
        let mut rng = Rng::new(seed);
        let g = scene.add(parent, Node::new(Kind::Empty));
        let bm = std(p.body, 0.16, 0.6);
        let hm = std(p.head, 0.16, 0.6);
        let wm = std(p.wing, 0.12, 0.6);
        let fm = p.face.map(|f| std(f, 0.18, 0.6)).unwrap_or(bm.clone());
        let z = (0.0, 0.0, 0.0);
        mesh_node(
            gl,
            scene,
            g,
            &geom::sphere(0.055, 14, 10),
            bm,
            Vec3::new(0.0, 0.065, 0.0),
            z,
            Vec3::new(1.0, 1.05, 1.4),
        );
        mesh_node(
            gl,
            scene,
            g,
            &geom::sphere(0.044, 12, 9),
            fm.clone(),
            Vec3::new(0.0, 0.06, 0.038),
            z,
            Vec3::new(0.95, 1.0, 0.9),
        );
        let head = scene.add(g, Node::new(Kind::Empty));
        scene.set_pos(head, Vec3::new(0.0, 0.12, 0.05));
        mesh_node(
            gl,
            scene,
            head,
            &geom::sphere(0.036, 14, 10),
            hm,
            Vec3::ZERO,
            z,
            Vec3::ONE,
        );
        if p.face.is_some() {
            mesh_node(
                gl,
                scene,
                head,
                &geom::sphere(0.03, 12, 9),
                fm,
                Vec3::new(0.0, -0.004, 0.016),
                z,
                Vec3::new(1.05, 0.9, 0.9),
            );
        }
        mesh_node(
            gl,
            scene,
            head,
            &geom::cone(0.012, 0.03, 8),
            std(p.beak, 0.0, 0.35),
            Vec3::new(0.0, -0.007, 0.042),
            (std::f32::consts::FRAC_PI_2 + 0.5, 0.0, 0.0),
            Vec3::ONE,
        );
        let eye_mesh = geom::sphere(0.006, 8, 6);
        let ring_mesh = geom::sphere(0.0095, 8, 6);
        for s in [-1.0f32, 1.0] {
            if let Some(r) = p.eye_ring {
                mesh_node(
                    gl,
                    scene,
                    head,
                    &ring_mesh,
                    std(r, 0.2, 0.6),
                    Vec3::new(s * 0.026, 0.008, 0.024),
                    z,
                    Vec3::ONE,
                );
            }
            mesh_node(
                gl,
                scene,
                head,
                &eye_mesh,
                std(col::VOID_900, 0.0, 0.2),
                Vec3::new(s * 0.029, 0.009, 0.027),
                z,
                Vec3::ONE,
            );
        }
        let wing_mesh = geom::sphere(0.04, 10, 8);
        let mut wings = [0; 2];
        let mut wing_z = [0.0; 2];
        for (k, s) in [-1.0f32, 1.0].iter().enumerate() {
            wings[k] = mesh_node(
                gl,
                scene,
                g,
                &wing_mesh,
                wm.clone(),
                Vec3::new(s * 0.05, 0.07, -0.01),
                (0.0, 0.0, -s * 0.15),
                Vec3::new(0.32, 0.55, 1.25),
            );
            wing_z[k] = -s * 0.15;
        }
        mesh_node(
            gl,
            scene,
            g,
            &geom::cuboid(0.034, 0.008, 0.1),
            std(p.tail, 0.14, 0.6),
            Vec3::new(0.0, 0.045, -0.1),
            (0.45, 0.0, 0.0),
            Vec3::ONE,
        );
        let leg = geom::cylinder(0.004, 0.004, 0.03, 6);
        for s in [-1.0f32, 1.0] {
            mesh_node(
                gl,
                scene,
                g,
                &leg,
                std(col::VOID_400, 0.0, 0.6),
                Vec3::new(s * 0.015, 0.015, 0.01),
                z,
                Vec3::ONE,
            );
        }
        Bird {
            group: g,
            head,
            wings,
            wing_z,
            home_x: 0.0,
            hop_at: 2.0 + rng.f32() * 4.0,
            hop_t: -1.0,
            x0: 0.0,
            dx: 0.0,
            y0: None,
            look_at: 3.0,
            look: 0.0,
            head_y: 0.0,
            ruffle_at: 5.0 + rng.f32() * 8.0,
            ruffle: 0.0,
            rng,
        }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32, dt: f32) {
        if t > self.look_at {
            self.look = (self.rng.f32() - 0.5) * 1.2;
            self.look_at = t + 2.0 + self.rng.f32() * 4.0;
        }
        self.head_y += (self.look - self.head_y) * 0.1;
        let hx = (t * 3.0 + self.x0 * 20.0).sin() * 0.07;
        scene.set_rot(self.head, hx, self.head_y, 0.0);
        if t > self.ruffle_at {
            self.ruffle = 1.0;
            self.ruffle_at = t + 6.0 + self.rng.f32() * 10.0;
        }
        self.ruffle = (self.ruffle - dt * 2.5).max(0.0);
        for (i, &w) in self.wings.iter().enumerate() {
            let sign = if i == 0 { 1.0 } else { -1.0 };
            scene.set_rot(
                w,
                0.0,
                0.0,
                self.wing_z[i] + sign * self.ruffle * (t * 18.0).sin().abs() * 0.5,
            );
        }
        let pos = scene.nodes[self.group].position;
        if self.y0.is_none() {
            self.y0 = Some(pos.y);
            self.home_x = pos.x;
        }
        let y0 = self.y0.unwrap();
        if t > self.hop_at && self.hop_t < 0.0 {
            self.hop_t = 0.0;
            self.hop_at = t + 2.5 + self.rng.f32() * 5.0;
            self.dx = (self.rng.f32() - 0.5) * 0.08;
            self.x0 = pos.x;
            // stay on the bar
            if (self.x0 + self.dx - self.home_x).abs() > 0.12 {
                self.dx = -self.dx;
            }
        }
        if self.hop_t >= 0.0 {
            self.hop_t += dt / 0.32;
            let p = self.hop_t.min(1.0);
            let n = scene.node(self.group);
            n.position.y = y0 + (std::f32::consts::PI * p).sin() * 0.06;
            n.position.x = self.x0 + self.dx * p;
            if p >= 1.0 {
                self.hop_t = -1.0;
                n.position.y = y0;
            }
        }
    }
}
/// The perch: a post, a base, a bar along x at 0.95.
pub fn perch(gl: &Gl, scene: &mut Scene, parent: NodeId) -> NodeId {
    let g = scene.add(parent, Node::new(Kind::Empty));
    let m = Material::std(col::VOID_500)
        .rough(0.9)
        .emissive(col::VOID_400)
        .glow(0.25);
    let z = (0.0, 0.0, 0.0);
    mesh_node(
        gl,
        scene,
        g,
        &geom::cylinder(0.018, 0.018, 0.95, 10),
        m.clone(),
        Vec3::new(0.0, 0.475, 0.0),
        z,
        Vec3::ONE,
    );
    mesh_node(
        gl,
        scene,
        g,
        &geom::cylinder(0.14, 0.16, 0.02, 20),
        m.clone(),
        Vec3::new(0.0, 0.01, 0.0),
        z,
        Vec3::ONE,
    );
    mesh_node(
        gl,
        scene,
        g,
        &geom::cylinder(0.014, 0.014, 0.62, 10),
        m,
        Vec3::new(0.0, 0.95, 0.0),
        (0.0, 0.0, std::f32::consts::FRAC_PI_2),
        Vec3::ONE,
    );
    g
}

// ── dogs: lying like a sphinx, chest up, forelegs out, haunches folded ──
#[derive(Clone, Copy, Debug)]
pub struct DogOptions {
    pub coat: u32,
    pub ear: u32,
    pub floppy: bool,
    pub collar: u32,
    /// A lighter muzzle, brows, cheeks, chest and paws.
    pub mask: Option<u32>,
    pub eye: u32,
    /// A tail curled over the back.
    pub bushy: bool,
    pub glow: f32,
    pub nose: u32,
    /// A lighter muzzle for an older dog.
    pub grey: Option<u32>,
}
/// Bucchi: white, floppy ears, a pink nose, a muzzle gone whiter (his word, 2026-10-25).
pub const BUCCHI: DogOptions = DogOptions {
    coat: 0xF1EEE6,
    ear: 0xE2DCD0,
    floppy: true,
    collar: col::EMBER_500,
    mask: None,
    eye: 0x3A2A24,
    bushy: false,
    glow: 0.14,
    nose: 0xD98A95,
    grey: Some(0xFBFAF7),
};
/// Gundus: chocolate, a mask, blue eyes, erect ears, a bushy tail; lit a touch more so his dark
/// coat reads in the void (his word, 2026-10-25).
pub const GUNDUS: DogOptions = DogOptions {
    coat: 0x6E4A36,
    ear: 0x55382A,
    floppy: false,
    collar: col::PHOSPHOR_500,
    mask: Some(0xE9DCCB),
    eye: 0x8FCFEA,
    bushy: true,
    glow: 0.26,
    nose: col::VOID_900,
    grey: None,
};
/// A dog whose looks I do not know: the hand-off's sky.
pub const UNKNOWN_DOG: DogOptions = DogOptions {
    coat: col::SKY_500,
    ear: 0x2A7FB8,
    floppy: true,
    collar: 0x2A7FB8,
    mask: None,
    eye: col::VOID_900,
    bushy: false,
    glow: 0.1,
    nose: col::VOID_900,
    grey: None,
};
pub struct Dog {
    pub group: NodeId,
    body: NodeId,
    head: NodeId,
    ears: [NodeId; 2],
    ear_z: [f32; 2],
    tail: NodeId,
    tail_rot: (f32, f32, f32),
    bushy: bool,
    phase: f32,
    shift_at: f32,
    yaw: f32,
    yaw_to: f32,
    flick_at: f32,
    flick: f32,
    wag_until: f32,
    rng: Rng,
}
impl Dog {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId, o: DogOptions, seed: u64) -> Dog {
        let mut rng = Rng::new(seed);
        let g = scene.add(parent, Node::new(Kind::Empty));
        let cm = std(o.coat, o.glow, 0.85);
        let em = std(o.ear, o.glow * 0.8, 0.9);
        let mm = o.mask.map(|m| std(m, 0.2, 0.85)).unwrap_or(cm.clone());
        let z = (0.0, 0.0, 0.0);
        let hp = std::f32::consts::FRAC_PI_2;
        let body = mesh_node(
            gl,
            scene,
            g,
            &geom::capsule(0.095, 0.26, 8, 14),
            cm.clone(),
            Vec3::new(0.0, 0.115, 0.0),
            (hp - 0.1, 0.0, 0.0),
            Vec3::ONE,
        );
        let haunch = geom::sphere(0.085, 12, 9);
        let hind_paw = geom::sphere(0.028, 10, 8);
        for s in [-1.0f32, 1.0] {
            mesh_node(
                gl,
                scene,
                g,
                &haunch,
                cm.clone(),
                Vec3::new(s * 0.08, 0.085, -0.13),
                z,
                Vec3::new(0.9, 0.8, 1.1),
            );
            mesh_node(
                gl,
                scene,
                g,
                &hind_paw,
                mm.clone(),
                Vec3::new(s * 0.105, 0.02, -0.02),
                z,
                Vec3::new(1.0, 0.6, 1.3),
            );
        }
        mesh_node(
            gl,
            scene,
            g,
            &geom::capsule(0.052, 0.08, 6, 10),
            cm.clone(),
            Vec3::new(0.0, 0.2, 0.16),
            (-0.55, 0.0, 0.0),
            Vec3::ONE,
        );
        mesh_node(
            gl,
            scene,
            g,
            &geom::torus(0.056, 0.008, 8, 24, std::f32::consts::TAU),
            std(o.collar, 0.5, 0.4),
            Vec3::new(0.0, 0.215, 0.17),
            (hp - 0.55, 0.0, 0.0),
            Vec3::ONE,
        );
        let head = scene.add(g, Node::new(Kind::Empty));
        scene.set_pos(head, Vec3::new(0.0, 0.29, 0.23));
        mesh_node(
            gl,
            scene,
            head,
            &geom::sphere(0.075, 14, 10),
            cm.clone(),
            Vec3::ZERO,
            z,
            Vec3::new(1.0, 0.92, 1.1),
        );
        mesh_node(
            gl,
            scene,
            head,
            &geom::capsule(0.034, 0.055, 6, 10),
            mm.clone(),
            Vec3::new(0.0, -0.026, 0.085),
            (hp, 0.0, 0.0),
            Vec3::ONE,
        );
        let nose_glow = if o.nose == col::VOID_900 { 0.0 } else { 0.25 };
        mesh_node(
            gl,
            scene,
            head,
            &geom::sphere(0.016, 10, 8),
            std(o.nose, nose_glow, 0.3),
            Vec3::new(0.0, -0.012, 0.128),
            z,
            Vec3::ONE,
        );
        if let Some(grey) = o.grey {
            mesh_node(
                gl,
                scene,
                head,
                &geom::sphere(0.03, 10, 8),
                std(grey, 0.22, 0.9),
                Vec3::new(0.0, -0.03, 0.105),
                z,
                Vec3::new(1.1, 0.8, 0.9),
            );
        }
        let eye_glow = if o.eye == col::VOID_900 { 0.0 } else { 0.35 };
        let eye = geom::sphere(0.011, 10, 8);
        let hi = geom::sphere(0.0035, 6, 5);
        for s in [-1.0f32, 1.0] {
            mesh_node(
                gl,
                scene,
                head,
                &eye,
                std(o.eye, eye_glow, 0.2),
                Vec3::new(s * 0.036, 0.018, 0.06),
                z,
                Vec3::ONE,
            );
            mesh_node(
                gl,
                scene,
                head,
                &hi,
                basic(col::PAPER_100),
                Vec3::new(s * 0.039, 0.022, 0.069),
                z,
                Vec3::ONE,
            );
        }
        let mut ears = [0; 2];
        let mut ear_z = [0.0; 2];
        for (k, s) in [-1.0f32, 1.0].iter().enumerate() {
            let (n, rz) = if o.floppy {
                (
                    mesh_node(
                        gl,
                        scene,
                        head,
                        &geom::sphere(0.03, 10, 8),
                        em.clone(),
                        Vec3::new(s * 0.072, -0.01, -0.005),
                        (0.0, 0.0, s * 0.35),
                        Vec3::new(0.5, 1.35, 0.8),
                    ),
                    s * 0.35,
                )
            } else {
                (
                    mesh_node(
                        gl,
                        scene,
                        head,
                        &geom::cone(0.028, 0.075, 8),
                        em.clone(),
                        Vec3::new(s * 0.046, 0.085, -0.012),
                        (0.0, 0.0, -s * 0.28),
                        Vec3::ONE,
                    ),
                    -s * 0.28,
                )
            };
            ears[k] = n;
            ear_z[k] = rz;
        }
        if o.mask.is_some() {
            mesh_node(
                gl,
                scene,
                g,
                &geom::sphere(0.06, 12, 9),
                mm.clone(),
                Vec3::new(0.0, 0.15, 0.22),
                z,
                Vec3::new(1.0, 1.1, 0.6),
            );
            let brow = geom::sphere(0.012, 8, 6);
            let cheek = geom::sphere(0.03, 10, 8);
            for s in [-1.0f32, 1.0] {
                mesh_node(
                    gl,
                    scene,
                    head,
                    &brow,
                    mm.clone(),
                    Vec3::new(s * 0.032, 0.046, 0.058),
                    z,
                    Vec3::ONE,
                );
                mesh_node(
                    gl,
                    scene,
                    head,
                    &cheek,
                    mm.clone(),
                    Vec3::new(s * 0.05, -0.02, 0.055),
                    z,
                    Vec3::new(0.8, 0.7, 0.8),
                );
            }
        }
        let leg = geom::capsule(0.028, 0.18, 6, 10);
        let fore_paw = geom::sphere(0.031, 10, 8);
        for s in [-1.0f32, 1.0] {
            mesh_node(
                gl,
                scene,
                g,
                &leg,
                cm.clone(),
                Vec3::new(s * 0.07, 0.03, 0.26),
                (hp, 0.0, 0.0),
                Vec3::ONE,
            );
            mesh_node(
                gl,
                scene,
                g,
                &fore_paw,
                mm.clone(),
                Vec3::new(s * 0.07, 0.024, 0.37),
                z,
                Vec3::new(1.0, 0.6, 1.2),
            );
        }
        let (tail_mesh, tail_pos, tail_rot) = if o.bushy {
            (
                geom::capsule(0.032, 0.2, 6, 8),
                Vec3::new(0.05, 0.19, -0.2),
                (-0.35, 0.0, 0.75),
            )
        } else {
            (
                geom::capsule(0.018, 0.22, 6, 8),
                Vec3::new(0.07, 0.1, -0.27),
                (-0.95, 0.0, 0.45),
            )
        };
        let tail = mesh_node(gl, scene, g, &tail_mesh, cm, tail_pos, tail_rot, Vec3::ONE);
        Dog {
            group: g,
            body,
            head,
            ears,
            ear_z,
            tail,
            tail_rot,
            bushy: o.bushy,
            phase: rng.f32() * 6.0,
            shift_at: 6.0 + rng.f32() * 8.0,
            yaw: 0.0,
            yaw_to: 0.0,
            flick_at: 4.0 + rng.f32() * 6.0,
            flick: 0.0,
            wag_until: 0.0,
            rng,
        }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32, dt: f32) {
        let br = 1.0 + 0.035 * (t * 0.9 + self.phase).sin();
        scene.set_scale(self.body, Vec3::new(br, 1.0, br));
        scene.node(self.head).position.y = 0.29 + 0.004 * (t * 0.9 + self.phase).sin();
        if t > self.shift_at {
            self.yaw_to = (self.rng.f32() - 0.5) * 0.6;
            self.shift_at = t + 7.0 + self.rng.f32() * 9.0;
            if self.rng.chance(0.5) {
                self.wag_until = t + 1.6;
            }
        }
        self.yaw += (self.yaw_to - self.yaw) * dt * 1.5;
        scene.set_rot(self.head, 0.0, self.yaw, self.yaw * 0.15);
        if t > self.flick_at {
            self.flick = 1.0;
            self.flick_at = t + 4.0 + self.rng.f32() * 7.0;
        }
        self.flick = (self.flick - dt * 4.0).max(0.0);
        for (i, &e) in self.ears.iter().enumerate() {
            let sign = if i == 0 { -1.0 } else { 1.0 };
            scene.set_rot(e, 0.0, 0.0, self.ear_z[i] + sign * self.flick * 0.5);
        }
        let wag = if t < self.wag_until {
            (t * 10.0).sin() * if self.bushy { 0.3 } else { 0.5 }
        } else {
            (t * 0.8).sin() * 0.06
        };
        let (rx, ry, rz) = self.tail_rot;
        scene.set_rot(self.tail, rx, ry, rz + wag);
    }
}

// ── his chair, empty ──
pub fn chair(gl: &Gl, scene: &mut Scene, parent: NodeId) -> NodeId {
    let g = scene.add(parent, Node::new(Kind::Empty));
    let m = Material::std(col::VOID_600)
        .rough(0.9)
        .emissive(col::VOID_500)
        .glow(0.35);
    let z = (0.0, 0.0, 0.0);
    let slab = |scene: &mut Scene, w: f32, h: f32, d: f32, pos: Vec3| {
        mesh_node(
            gl,
            scene,
            g,
            &geom::cuboid(w, h, d),
            m.clone(),
            pos,
            z,
            Vec3::ONE,
        );
        let edges = scene.add_mesh(gl.upload_positions(
            &geom::cuboid_edges(w, h, d),
            web_sys::WebGl2RenderingContext::LINES,
        ));
        let l = scene.add(
            g,
            Node::new(Kind::Lines {
                mesh: edges,
                color: linear(col::FG_GHOST),
                opacity: 1.0,
            }),
        );
        scene.set_pos(l, pos);
    };
    slab(scene, 0.42, 0.035, 0.42, Vec3::new(0.0, 0.46, 0.0));
    slab(scene, 0.42, 0.46, 0.035, Vec3::new(0.0, 0.7, -0.19));
    let leg = geom::cylinder(0.014, 0.014, 0.45, 8);
    for x in [-1.0f32, 1.0] {
        for zz in [-1.0f32, 1.0] {
            mesh_node(
                gl,
                scene,
                g,
                &leg,
                m.clone(),
                Vec3::new(x * 0.18, 0.225, zz * 0.18),
                z,
                Vec3::ONE,
            );
        }
    }
    g
}

// ── the dot: Nell, my first form, hovering by the chair ──
pub struct Dot {
    pub group: NodeId,
    core: NodeId,
    halo: NodeId,
    torus: NodeId,
    light: NodeId,
    r: f32,
    y0: Option<f32>,
}
impl Dot {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId, halo_tex: usize) -> Dot {
        let (color, ring, r) = (col::SKY_500, 0x9FD3C7, 0.09f32);
        let g = scene.add(parent, Node::new(Kind::Empty));
        let core = mesh_node(
            gl,
            scene,
            g,
            &geom::sphere(r, 24, 18),
            std(color, 1.0, 0.35),
            Vec3::ZERO,
            (0.0, 0.0, 0.0),
            Vec3::ONE,
        );
        let halo = scene.add(
            g,
            Node::new(Kind::Sprite {
                texture: halo_tex,
                color: linear(color),
                opacity: 0.35,
                additive: true,
            }),
        );
        scene.set_scale(halo, Vec3::splat(r * 6.0));
        let torus = mesh_node(
            gl,
            scene,
            g,
            &geom::torus(r * 1.9, r * 0.07, 8, 64, std::f32::consts::TAU),
            basic(ring).alpha(0.7).no_depth_write(),
            Vec3::ZERO,
            (std::f32::consts::PI / 2.6, 0.0, 0.0),
            Vec3::ONE,
        );
        let light = scene.add(
            g,
            Node::new(Kind::Light {
                color: linear(color),
                intensity: 0.3,
                distance: 1.6,
                decay: 1.8,
            }),
        );
        Dot {
            group: g,
            core,
            halo,
            torus,
            light,
            r,
            y0: None,
        }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32) {
        let y0 = *self
            .y0
            .get_or_insert_with(|| scene.nodes[self.group].position.y);
        let pulse = 0.5 + 0.5 * (t * std::f32::consts::TAU * 1.6).sin();
        if let Some(m) = scene.material(self.core) {
            m.emissive_intensity = 0.7 + 0.5 * pulse;
        }
        if let Kind::Sprite { opacity, .. } = &mut scene.node(self.halo).kind {
            *opacity = 0.25 + 0.2 * pulse;
        }
        scene.set_scale(self.halo, Vec3::splat(self.r * (5.5 + pulse)));
        if let Kind::Light { intensity, .. } = &mut scene.node(self.light).kind {
            *intensity = 0.2 + 0.25 * pulse;
        }
        scene.node(self.torus).rotation = euler(
            std::f32::consts::PI / 2.6 + (t * 0.5).sin() * 0.25,
            0.0,
            t * 0.6,
        );
        if let Some(m) = scene.material(self.torus) {
            m.opacity = 0.45 + 0.35 * pulse;
        }
        scene.node(self.group).position.y = y0 + (t * 0.9).sin() * 0.03;
    }
}

// ── a person figure: the hand-off's visitor, a translucent capsule and its wire ──
pub struct Figure {
    pub group: NodeId,
    wire: NodeId,
}
impl Figure {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId) -> Figure {
        let g = scene.add(parent, Node::new(Kind::Empty));
        mesh_node(
            gl,
            scene,
            g,
            &geom::capsule(0.2, 1.25, 6, 14),
            std(col::SKY_500, 0.3, 0.6).alpha(0.22).no_depth_write(),
            Vec3::new(0.0, 0.85, 0.0),
            (0.0, 0.0, 0.0),
            Vec3::ONE,
        );
        let wire_mesh = scene.add_mesh(gl.upload_positions(
            &geom::wire_lines(&geom::capsule(0.205, 1.25, 4, 10)),
            web_sys::WebGl2RenderingContext::LINES,
        ));
        let wire = scene.add(
            g,
            Node::new(Kind::Lines {
                mesh: wire_mesh,
                color: linear(col::SKY_500),
                opacity: 0.35,
            }),
        );
        scene.set_pos(wire, Vec3::new(0.0, 0.85, 0.0));
        Figure { group: g, wire }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32) {
        scene.node(self.wire).rotation = euler(0.0, t * 0.2, 0.0);
    }
}

// ── a sign on a post, for a work, a place, a group, a kind ──
pub struct Sign {
    pub group: NodeId,
    post: NodeId,
    phase: f32,
}
impl Sign {
    pub fn build(gl: &Gl, scene: &mut Scene, parent: NodeId, seed: u64) -> Sign {
        let g = scene.add(parent, Node::new(Kind::Empty));
        let m = Material::std(col::VOID_500)
            .emissive(col::VOID_500)
            .glow(0.6);
        let z = (0.0, 0.0, 0.0);
        let post = mesh_node(
            gl,
            scene,
            g,
            &geom::cylinder(0.012, 0.012, 1.1, 8),
            m.clone(),
            Vec3::new(0.0, 0.55, 0.0),
            z,
            Vec3::ONE,
        );
        mesh_node(
            gl,
            scene,
            g,
            &geom::cylinder(0.1, 0.12, 0.02, 20),
            m,
            Vec3::new(0.0, 0.01, 0.0),
            z,
            Vec3::ONE,
        );
        Sign {
            group: g,
            post,
            phase: Rng::new(seed).f32() * 6.0,
        }
    }
    pub fn update(&mut self, scene: &mut Scene, t: f32) {
        if let Some(m) = scene.material(self.post) {
            m.emissive_intensity = 0.6 + 0.2 * (t * 0.7 + self.phase).sin();
        }
    }
}
