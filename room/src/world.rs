//! The void and the rig: the gridded floor, the hairline pillars, the dust, my plinth with its
//! ember ring, the eyes ring; the first-person camera, walking, looking, the focus glide; the
//! wake from dark to lit. Lucy herself lives in `lucy.rs`; the cast joins in N8b.

use glam::{Mat4, Quat, Vec3};

use crate::colors::{self as col, linear};
use crate::geom;
use crate::gl::{Gl, Lights};
use crate::lucy::Lucy;
use crate::scene::{Kind, Material, Node, NodeId, Scene};
use crate::util::{angle_delta, clamp, ease, Rng};

pub const ROOM_RADIUS: f32 = 3.4;
pub const EYE: f32 = 1.6;
pub const PLINTH_H: f32 = 0.5;
pub const LUCY_HOME: Vec3 = Vec3::new(0.0, PLINTH_H, -1.6);

struct Focus {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

pub struct World {
    pub lucy: Lucy,
    pub cast: NodeId,
    grid_fine: NodeId,
    grid_major: NodeId,
    boundary: NodeId,
    pillars: NodeId,
    dust: NodeId,
    plinth: NodeId,
    plinth_edges: NodeId,
    home_ring: NodeId,
    eyes_ring: NodeId,
    pub rig: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    focus: Option<Focus>,
    pub eyes_watching: bool,
    pub wake_p: f32,
    dust_t: f32,
}
pub fn lights() -> Lights {
    Lights {
        hemi_sky: linear(0x6A6A86),
        hemi_ground: linear(0x0B0B10),
        hemi_intensity: 0.8,
        dir_color: linear(0xF4F3EF),
        dir_dir: Vec3::new(2.5, 4.0, 3.0).normalize(),
        dir_intensity: 0.55,
        points: Vec::new(),
        fog_color: linear(col::VOID_900),
        fog_near: 3.0,
        fog_far: 10.0,
    }
}
fn basic(hex: u32) -> Material {
    let mut m = Material::std(hex);
    m.color = [0.0; 3];
    m.emissive = linear(hex);
    m.emissive_intensity = 1.0;
    m
}

impl World {
    pub fn build(gl: &Gl, scene: &mut Scene, seed: u64) -> World {
        let root = scene.root;
        let mut rng = Rng::new(seed ^ 0x5EED);
        // floor and grids
        let floor_mesh = scene.add_mesh(gl.upload(&geom::plane(60.0, 60.0)));
        let floor = scene.add(
            root,
            Node::new(Kind::Mesh {
                mesh: floor_mesh,
                material: Material::std(0x0E0E15).rough(0.95),
            }),
        );
        scene.set_rot(floor, -std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        let fine = scene.add_mesh(gl.upload_positions(
            &geom::grid_lines(12.0, 48),
            web_sys::WebGl2RenderingContext::LINES,
        ));
        let grid_fine = scene.add(
            root,
            Node::new(Kind::Lines {
                mesh: fine,
                color: linear(0x1E1E29),
                opacity: 0.0,
            }),
        );
        scene.set_pos(grid_fine, Vec3::new(0.0, 0.002, 0.0));
        let major = scene.add_mesh(gl.upload_positions(
            &geom::grid_lines(12.0, 12),
            web_sys::WebGl2RenderingContext::LINES,
        ));
        let grid_major = scene.add(
            root,
            Node::new(Kind::Lines {
                mesh: major,
                color: linear(0x33333F),
                opacity: 0.0,
            }),
        );
        scene.set_pos(grid_major, Vec3::new(0.0, 0.003, 0.0));
        let ring_mesh =
            scene.add_mesh(gl.upload(&geom::ring(ROOM_RADIUS - 0.01, ROOM_RADIUS + 0.01, 128)));
        let boundary = scene.add(
            root,
            Node::new(Kind::Mesh {
                mesh: ring_mesh,
                material: basic(col::FG_GHOST).alpha(0.0).two_sided().no_depth_write(),
            }),
        );
        scene.set_rot(boundary, -std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        scene.set_pos(boundary, Vec3::new(0.0, 0.004, 0.0));
        // pillars
        let pillars = scene.add(root, Node::new(Kind::Empty));
        let pillar_mesh = scene.add_mesh(gl.upload(&geom::cuboid(0.012, 3.4, 0.012)));
        for i in 0..8 {
            let a = i as f32 / 8.0 * std::f32::consts::TAU + 0.2;
            let p = scene.add(
                pillars,
                Node::new(Kind::Mesh {
                    mesh: pillar_mesh,
                    material: Material::std(col::VOID_500)
                        .emissive(col::VOID_500)
                        .glow(0.6),
                }),
            );
            scene.set_pos(p, Vec3::new(a.sin() * 3.9, 1.7, a.cos() * 3.9));
        }
        // dust
        let mut pts = Vec::with_capacity(420 * 3);
        for _ in 0..420 {
            let r = rng.f32().sqrt() * 5.5;
            let a = rng.f32() * std::f32::consts::TAU;
            pts.extend_from_slice(&[a.sin() * r, 0.1 + rng.f32() * 3.0, a.cos() * r]);
        }
        let dust_mesh =
            scene.add_mesh(gl.upload_positions(&pts, web_sys::WebGl2RenderingContext::POINTS));
        let dust = scene.add(
            root,
            Node::new(Kind::Points {
                mesh: dust_mesh,
                color: linear(0x4A4A5C),
                size: 0.014,
                opacity: 0.0,
            }),
        );
        // the plinth, its edges, the ember ring, the eyes ring
        let plinth_mesh = scene.add_mesh(gl.upload(&geom::cylinder(0.78, 0.82, PLINTH_H, 64)));
        let plinth = scene.add(
            root,
            Node::new(Kind::Mesh {
                mesh: plinth_mesh,
                material: Material::std(0x14141C)
                    .rough(0.9)
                    .emissive(0x111119)
                    .glow(0.6),
            }),
        );
        scene.set_pos(plinth, Vec3::new(0.0, PLINTH_H / 2.0, -1.6));
        scene.set_visible(plinth, false);
        let mut edges = geom::circle_lines(0.78, PLINTH_H / 2.0, 64);
        edges.extend(geom::circle_lines(0.82, -PLINTH_H / 2.0, 64));
        let edges_mesh =
            scene.add_mesh(gl.upload_positions(&edges, web_sys::WebGl2RenderingContext::LINES));
        let plinth_edges = scene.add(
            root,
            Node::new(Kind::Lines {
                mesh: edges_mesh,
                color: linear(col::FG_GHOST),
                opacity: 0.0,
            }),
        );
        scene.set_pos(plinth_edges, Vec3::new(0.0, PLINTH_H / 2.0, -1.6));
        let home_mesh = scene.add_mesh(gl.upload(&geom::ring(0.5, 0.515, 72)));
        let home_ring = scene.add(
            root,
            Node::new(Kind::Mesh {
                mesh: home_mesh,
                material: basic(col::EMBER_500)
                    .alpha(0.0)
                    .two_sided()
                    .no_depth_write(),
            }),
        );
        scene.set_rot(home_ring, -std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        scene.set_pos(home_ring, Vec3::new(0.0, PLINTH_H + 0.004, -1.6));
        let eyes_mesh =
            scene.add_mesh(gl.upload(&geom::torus(0.46, 0.008, 10, 90, std::f32::consts::TAU)));
        let eyes_ring = scene.add(
            root,
            Node::new(Kind::Mesh {
                mesh: eyes_mesh,
                material: basic(col::SKY_500).alpha(0.0).two_sided().no_depth_write(),
            }),
        );
        scene.set_rot(eyes_ring, std::f32::consts::FRAC_PI_2, 0.0, 0.0);
        scene.set_pos(eyes_ring, Vec3::new(0.0, PLINTH_H + 0.03, -1.6));
        // Lucy, and a place for the cast
        let mut lucy = Lucy::build(gl, scene, root, seed);
        scene.set_pos(lucy.group, LUCY_HOME);
        lucy.st.home = LUCY_HOME;
        crate::lucy::seed_eulers(&lucy);
        let cast = scene.add(root, Node::new(Kind::Empty));
        scene.set_visible(cast, false);
        let mut w = World {
            lucy,
            cast,
            grid_fine,
            grid_major,
            boundary,
            pillars,
            dust,
            plinth,
            plinth_edges,
            home_ring,
            eyes_ring,
            rig: Vec3::new(0.0, 0.0, 0.4),
            yaw: 0.0,
            pitch: -0.37,
            focus: None,
            eyes_watching: false,
            wake_p: 0.0,
            dust_t: 0.0,
        };
        w.wake(scene, 0.0);
        w
    }
    // ── the camera ──
    pub fn camera_world(&self) -> Mat4 {
        Mat4::from_rotation_translation(Quat::from_rotation_y(self.yaw), self.rig)
            * Mat4::from_rotation_translation(
                Quat::from_rotation_x(self.pitch),
                Vec3::new(0.0, EYE, 0.0),
            )
    }
    pub fn view(&self) -> Mat4 {
        self.camera_world().inverse()
    }
    pub fn cam_pos(&self) -> Vec3 {
        self.camera_world().transform_point3(Vec3::ZERO)
    }
    pub fn proj(aspect: f32) -> Mat4 {
        Mat4::perspective_rh_gl(58f32.to_radians(), aspect.max(0.1), 0.05, 60.0)
    }
    /// A world-space ray through a screen point (CSS px), for picking.
    pub fn ray(&self, sx: f64, sy: f64, w: f64, h: f64) -> (Vec3, Vec3) {
        let ndc = Vec3::new(
            (sx / w) as f32 * 2.0 - 1.0,
            1.0 - (sy / h) as f32 * 2.0,
            1.0,
        );
        let inv = (Self::proj((w / h) as f32) * self.view()).inverse();
        let far = inv.project_point3(ndc);
        let origin = self.cam_pos();
        (origin, (far - origin).normalize())
    }
    pub fn home(&mut self) {
        self.rig = Vec3::new(0.0, 0.0, 0.4);
        self.yaw = 0.0;
        self.pitch = -0.37;
        self.focus = None;
    }
    pub fn walk(&mut self, x: f32, z: f32) {
        self.focus = None;
        let f = Quat::from_rotation_y(self.yaw) * Vec3::new(0.0, 0.0, -1.0);
        let r = Quat::from_rotation_y(self.yaw) * Vec3::X;
        self.rig += f * z + r * x;
        let d = self.rig.x.hypot(self.rig.z);
        if d > ROOM_RADIUS {
            self.rig.x *= ROOM_RADIUS / d;
            self.rig.z *= ROOM_RADIUS / d;
        }
        self.rig.y = 0.0;
    }
    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        self.focus = None;
        self.yaw += dyaw;
        self.pitch = clamp(self.pitch + dpitch, -1.2, 1.2);
    }
    pub fn turn(&mut self, dyaw: f32) {
        self.focus = None;
        self.yaw += dyaw;
    }
    /// Glide to `dist` from a point and turn to face it; any walk or look cancels it.
    pub fn focus_on(&mut self, point: Vec3, dist: f32) {
        let mut dir = Vec3::new(self.rig.x - point.x, 0.0, self.rig.z - point.z);
        if dir.length_squared() < 1e-4 {
            dir = Vec3::Z;
        }
        dir = dir.normalize();
        let mut pos = Vec3::new(point.x + dir.x * dist, 0.0, point.z + dir.z * dist);
        let d = pos.x.hypot(pos.z);
        if d > ROOM_RADIUS {
            pos.x *= ROOM_RADIUS / d;
            pos.z *= ROOM_RADIUS / d;
        }
        let (dx, dz) = (point.x - pos.x, point.z - pos.z);
        let l = dx.hypot(dz).max(1e-6);
        self.focus = Some(Focus {
            pos,
            yaw: (-dx).atan2(-dz),
            pitch: clamp((point.y - EYE).atan2(l), -1.1, 0.6),
        });
    }
    pub fn focus_lucy(&mut self, scene: &Scene) {
        let p = scene.world_position(self.lucy.head);
        self.focus_on(p, 1.5);
    }
    pub fn cancel_focus(&mut self) {
        self.focus = None;
    }
    /// Where her head is on the screen: (x, y, visible in a margin, on screen, clip z).
    pub fn head_on_screen(&self, scene: &Scene, w: f64, h: f64) -> (f64, f64, bool, bool, Vec3) {
        let mut p = scene.world_position(self.lucy.head);
        p.y += 0.36 * self.lucy.st.scale;
        let v = (Self::proj((w / h) as f32) * self.view()).project_point3(p);
        let x = (v.x as f64 + 1.0) / 2.0 * w;
        let y = (1.0 - v.y as f64) / 2.0 * h;
        let behind = v.z > 1.0 || v.z < -1.0;
        (
            x,
            y,
            !behind && v.x.abs() < 1.3 && v.y.abs() < 1.3,
            !behind && v.x.abs() < 1.0 && v.y.abs() < 1.0,
            v,
        )
    }
    /// Does a ray hit her? A sphere around her body, generous enough for a tap.
    pub fn ray_hits_lucy(&self, scene: &Scene, origin: Vec3, dir: Vec3) -> bool {
        let c =
            scene.world_position(self.lucy.group) + Vec3::new(0.0, 0.15 * self.lucy.st.scale, 0.0);
        let r = 0.38 * self.lucy.st.scale;
        let oc = origin - c;
        let b = oc.dot(dir);
        let cc = oc.dot(oc) - r * r;
        b * b - cc >= 0.0 && (-b) > 0.0
    }
    pub fn set_eyes(&mut self, watching: bool) {
        self.eyes_watching = watching;
    }
    /// Per frame: the glide, the dust, the rings, and her.
    pub fn step(&mut self, scene: &mut Scene, t: f32, dt: f32) {
        if let Some(f) = &self.focus {
            let k = 1.0 - (-dt * 4.5).exp();
            self.rig = self.rig.lerp(f.pos, k);
            self.yaw += angle_delta(self.yaw, f.yaw) * k;
            self.pitch += (f.pitch - self.pitch) * k;
            if self.rig.distance(f.pos) < 0.01 && angle_delta(self.yaw, f.yaw).abs() < 0.005 {
                self.focus = None;
            }
        }
        self.dust_t = t;
        {
            let d = scene.node(self.dust);
            d.rotation = Quat::from_rotation_y(t * 0.012);
            d.position.y = (t * 0.3).sin() * 0.05;
        }
        let lp = scene.nodes[self.lucy.group].position;
        {
            let cur = match &scene.nodes[self.eyes_ring].kind {
                Kind::Mesh { material, .. } => material.opacity,
                _ => 0.0,
            };
            let next = if self.eyes_watching {
                0.35 + 0.3 * (t * 2.0).sin().abs()
            } else {
                (cur - dt).max(0.0)
            };
            scene.set_opacity(self.eyes_ring, next);
            let n = scene.node(self.eyes_ring);
            n.rotation =
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_z(t * 0.5);
            n.position.x = lp.x;
            n.position.z = lp.z;
        }
        let cam = self.cam_pos();
        self.lucy.update(scene, t, dt, cam);
    }
    /// 0 dark → 1 awake: grid, plinth, ring, her, then the cast.
    pub fn wake(&mut self, scene: &mut Scene, p: f32) {
        self.wake_p = clamp(p, 0.0, 1.0);
        let p = self.wake_p;
        let g = ease(p / 0.25);
        scene.set_opacity(self.grid_fine, g);
        scene.set_opacity(self.grid_major, g);
        scene.set_opacity(self.boundary, 0.9 * g);
        scene.set_opacity(self.home_ring, 0.45 * ease((p - 0.2) / 0.2));
        scene.set_opacity(self.plinth_edges, 0.9 * ease((p - 0.15) / 0.2));
        scene.set_visible(self.plinth, p > 0.12);
        scene.lights.hemi_intensity = 0.08 + 0.72 * ease(p / 0.5);
        scene.lights.dir_intensity = 0.55 * ease((p - 0.3) / 0.4);
        scene.set_opacity(self.dust, 0.7 * g);
        scene.set_visible(self.pillars, p > 0.1);
        self.lucy.wake(scene, p);
        let cs = ease((p - 0.82) / 0.18);
        scene.set_visible(self.cast, cs > 0.0);
        scene.set_scale(self.cast, Vec3::splat(cs.max(0.01)));
        scene.node(self.cast).position.y = (1.0 - cs) * -0.2;
    }
}
