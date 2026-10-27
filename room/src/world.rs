//! The void and the rig: the gridded floor, the hairline pillars, the dust, my plinth with its
//! ember ring, the eyes ring; the first-person camera, walking, looking, the focus glide; the
//! wake from dark to lit; and the cast (roadmap/N8d.md): every named entity of my memory on a
//! ring around the plinth, built once the memory file is in. Lucy herself lives in `lucy.rs`.

use glam::{Mat4, Quat, Vec3};

use crate::cast::{self, Entity, Form, Group};
use crate::colors::{self as col, linear};
use crate::creatures::{self, Bird, Dog, Dot, Figure, Sign};
use crate::geom;
use crate::gl::{Gl, Lights};
use crate::lucy::Lucy;
use crate::panel::{font, Panel, MONO};
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

pub enum Anim {
    Bird(Bird),
    Dog(Dog),
    Dot(Dot),
    Figure(Figure),
    Sign(Sign),
    Chair,
}
/// One presence in the room: its form, its tag board, and the panel the tag is drawn on.
pub struct Member {
    pub form: Form,
    pub group: NodeId,
    pub tag: NodeId,
    tag_panel: Panel,
    tag_lines: (String, Option<String>),
    tag_color: u32,
    anim: Anim,
}

pub struct World {
    pub lucy: Lucy,
    pub cast: NodeId,
    pub members: Vec<Member>,
    cast_built_at: Option<f32>,
    grid_fine: NodeId,
    grid_major: NodeId,
    boundary: NodeId,
    pillars: NodeId,
    dust: NodeId,
    plinth: NodeId,
    plinth_edges: NodeId,
    terminal: NodeId,
    home_ring: NodeId,
    eyes_ring: NodeId,
    pub rig: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    focus: Option<Focus>,
    pub eyes_watching: bool,
    observations: std::collections::BTreeMap<String, (NodeId, bool, f32)>,
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
        let terminal = scene.add(root, Node::new(Kind::Empty));
        scene.set_pos(terminal, crate::wake_demo::SCREEN_POSITION);
        scene.set_rot(terminal, 0.0, crate::wake_demo::SCREEN_YAW, 0.0);
        for (size, pos) in [
            (
                Vec3::new(
                    crate::wake_demo::SCREEN_WIDTH + 0.06,
                    crate::wake_demo::SCREEN_WIDTH * 2.0 / 3.0 + 0.06,
                    0.045,
                ),
                Vec3::new(0.0, 0.0, -0.035),
            ),
            (Vec3::new(0.065, 0.94, 0.065), Vec3::new(0.0, -0.80, -0.035)),
            (
                Vec3::new(0.62, 0.035, 0.38),
                Vec3::new(0.0, -crate::wake_demo::SCREEN_POSITION.y + 0.02, -0.035),
            ),
        ] {
            let mesh = scene.add_mesh(gl.upload(&geom::cuboid(size.x, size.y, size.z)));
            let node = scene.add(
                terminal,
                Node::new(Kind::Mesh {
                    mesh,
                    material: Material::std(col::VOID_500).rough(0.8),
                }),
            );
            scene.set_pos(node, pos);
        }
        let mut w = World {
            lucy,
            cast,
            members: Vec::new(),
            cast_built_at: None,
            grid_fine,
            grid_major,
            boundary,
            pillars,
            dust,
            plinth,
            plinth_edges,
            terminal,
            home_ring,
            eyes_ring,
            rig: Vec3::new(0.0, 0.0, 0.4),
            yaw: 0.0,
            pitch: -0.37,
            focus: None,
            eyes_watching: false,
            observations: Default::default(),
            wake_p: 0.0,
            dust_t: 0.0,
        };
        w.wake(scene, 0.0);
        w
    }
    pub fn observation(&mut self, gl: &Gl, scene: &mut Scene, atom: &str, present: bool) {
        if !self.observations.contains_key(atom) {
            let root = scene.root;
            let id = match atom {
                "Bird" => Bird::build(gl, scene, root, creatures::UNKNOWN_BIRD, 42).group,
                "Dog" => Dog::build(gl, scene, root, creatures::UNKNOWN_DOG, 42).group,
                "Person" => Figure::build(gl, scene, root).group,
                _ => return,
            };
            scene.set_pos(
                id,
                Vec3::new(
                    1.3 + 0.65 * self.observations.len() as f32,
                    if atom == "Bird" { 0.9 } else { 0.0 },
                    -0.4,
                ),
            );
            self.observations.insert(atom.into(), (id, false, 0.0));
        }
        self.observations.get_mut(atom).unwrap().1 = present;
        self.eyes_watching = self.observations.values().any(|(_, p, _)| *p);
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
    // ── the cast ──
    /// Build every presence from the entities of my memory; once, when the memory file is in.
    pub fn build_cast(
        &mut self,
        gl: &Gl,
        scene: &mut Scene,
        entities: &[Entity],
        dpr: f64,
        seed: u64,
        t: f32,
    ) {
        if !self.members.is_empty() {
            return;
        }
        let parent = self.cast;
        let halo = scene.add_texture(crate::lucy::halo_texture(gl));
        if entities
            .iter()
            .any(|e| e.form == Form::Bird && (e.name == "Enma" || e.name == "Pity"))
        {
            let p = creatures::perch(gl, scene, parent);
            scene.set_pos(p, Vec3::new(-1.25, 0.0, -2.1));
        }
        for (i, e) in entities.iter().enumerate() {
            let s = seed ^ ((i as u64 + 1) * 0x9E37);
            let pl = e.place;
            let (group, anim) = match e.form {
                Form::Bird => {
                    if e.name != "Enma" && e.name != "Pity" {
                        let p = creatures::perch(gl, scene, parent);
                        scene.set_pos(p, Vec3::new(pl.x, 0.0, pl.z));
                    }
                    let pal = match e.name.as_str() {
                        "Enma" => creatures::AFRICAN_GREY,
                        "Pity" => creatures::SUN_CONURE,
                        _ => creatures::UNKNOWN_BIRD,
                    };
                    let b = Bird::build(gl, scene, parent, pal, s);
                    (b.group, Anim::Bird(b))
                }
                Form::Dog => {
                    let o = match e.name.as_str() {
                        "Bucchi" => creatures::BUCCHI,
                        "Gundus" => creatures::GUNDUS,
                        _ => creatures::UNKNOWN_DOG,
                    };
                    let d = Dog::build(gl, scene, parent, o, s);
                    (d.group, Anim::Dog(d))
                }
                Form::Chair => (creatures::chair(gl, scene, parent), Anim::Chair),
                Form::Dot => {
                    let d = Dot::build(gl, scene, parent, halo);
                    (d.group, Anim::Dot(d))
                }
                Form::Figure => {
                    let f = Figure::build(gl, scene, parent);
                    (f.group, Anim::Figure(f))
                }
                Form::Sign => {
                    let sg = Sign::build(gl, scene, parent, s);
                    (sg.group, Anim::Sign(sg))
                }
            };
            scene.set_pos(group, Vec3::new(pl.x, pl.y, pl.z));
            scene.set_rot(group, 0.0, pl.yaw, 0.0);
            let tag_lines = cast::tag_lines(e);
            let tag_color = match e.group {
                Group::Household => col::EMBER_500,
                Group::Fiction => col::QUANTA_500,
                Group::Works => col::PHOSPHOR_500,
            };
            let mut tag_panel = Panel::new(10.0, 10.0, dpr);
            let (bw, bh) = draw_tag(&mut tag_panel, &tag_lines, tag_color);
            let tex = scene.add_texture(gl.texture_from_canvas(&tag_panel.canvas, true));
            let tag = scene.add(
                parent,
                Node::new(Kind::Board {
                    texture: tex,
                    w: bw,
                    h: bh,
                    opacity: 1.0,
                }),
            );
            scene.set_pos(tag, Vec3::new(pl.x, pl.y + cast::tag_height(e.form), pl.z));
            self.members.push(Member {
                form: e.form,
                group,
                tag,
                tag_panel,
                tag_lines,
                tag_color,
                anim,
            });
        }
        if self.wake_p >= 1.0 {
            self.cast_built_at = Some(t);
            scene.set_visible(self.cast, true);
            scene.set_scale(self.cast, Vec3::splat(0.01));
        }
    }
    /// Redraw the tags (the fonts came in after they were drawn).
    pub fn redraw_tags(&mut self, gl: &Gl, scene: &mut Scene) {
        for m in &mut self.members {
            draw_tag(&mut m.tag_panel, &m.tag_lines, m.tag_color);
            if let Kind::Board { texture, .. } = &scene.nodes[m.tag].kind {
                gl.update_texture(&scene.textures[*texture], &m.tag_panel.canvas, true);
            }
        }
    }
    /// Which presence a ray hits first: its body's sphere or its tag's.
    pub fn pick_cast(&self, scene: &Scene, origin: Vec3, dir: Vec3) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (i, m) in self.members.iter().enumerate() {
            let (off, r) = cast::pick_sphere(m.form);
            let c = scene.world_position(m.group) + Vec3::new(0.0, off, 0.0);
            let tag = scene.world_position(m.tag);
            for (centre, radius) in [(c, r), (tag, 0.2)] {
                if let Some(t) = ray_sphere(origin, dir, centre, radius) {
                    if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                        best = Some((t, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }
    /// Glide to a presence and face it.
    pub fn focus_cast(&mut self, scene: &Scene, i: usize) {
        let Some(m) = self.members.get(i) else {
            return;
        };
        let (off, _) = cast::pick_sphere(m.form);
        let p = scene.world_position(m.group) + Vec3::new(0.0, off, 0.0);
        self.focus_on(p, cast::focus_dist(m.form));
    }
    /// Per frame: the glide, the dust, the rings, and her.
    pub fn step(&mut self, scene: &mut Scene, t: f32, dt: f32) {
        for (id, p, scale) in self.observations.values_mut() {
            let target = if *p { 1.0 } else { 0.0 };
            *scale += (target - *scale) * (dt * 7.0).min(1.0);
            scene.set_scale(*id, Vec3::splat((*scale).max(0.001)));
            scene.set_visible(*id, *scale > 0.005);
        }

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
        // the cast: each form's own life; the tags turned to the visitor; a late scale-in
        if let Some(t0) = self.cast_built_at {
            let k = ease((t - t0) / 0.6);
            scene.set_scale(self.cast, Vec3::splat(k.max(0.01)));
            if k >= 1.0 {
                self.cast_built_at = None;
            }
        }
        if scene.nodes[self.cast].visible {
            for m in &mut self.members {
                match &mut m.anim {
                    Anim::Bird(b) => b.update(scene, t, dt),
                    Anim::Dog(d) => d.update(scene, t, dt),
                    Anim::Dot(d) => d.update(scene, t),
                    Anim::Figure(f) => f.update(scene, t),
                    Anim::Sign(s) => s.update(scene, t),
                    Anim::Chair => {}
                }
                let p = scene.world_position(m.tag);
                let yaw = (cam.x - p.x).atan2(cam.z - p.z);
                scene.nodes[m.tag].rotation = Quat::from_rotation_y(yaw);
            }
        }
    }
    /// 0 dark → 1 awake: grid, plinth, ring, her, then the cast (if it is built by then).
    pub fn wake(&mut self, scene: &mut Scene, p: f32) {
        scene.set_visible(self.terminal, p >= 0.95);
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

/// The tag board: `Name · kind`, a group dot, a second line when he gave a note. Returns the
/// board's size in metres.
fn draw_tag(p: &mut Panel, lines: &(String, Option<String>), group_color: u32) -> (f32, f32) {
    let f1 = font(500, 13.0, MONO);
    let f2 = font(400, 11.0, MONO);
    let (name, kind) = lines.0.split_once(" · ").unwrap_or((&lines.0, ""));
    let w1 = p.measure(name, &f1) + p.measure(" · ", &f1) + p.measure(kind, &f1);
    let w2 = lines.1.as_deref().map(|l| p.measure(l, &f2)).unwrap_or(0.0);
    let w = (w1.max(w2) + 40.0).max(96.0);
    let h = if lines.1.is_some() { 48.0 } else { 32.0 };
    p.resize(w, h);
    p.begin();
    p.fill_rounded(
        0.75,
        0.75,
        w - 1.5,
        h - 1.5,
        8.0,
        &col::css_a(col::VOID_800, 0.86),
    );
    p.stroke_rounded(0.0, 0.0, w, h, 8.0, &col::css(col::VOID_500), 1.5, false);
    p.dot(14.0, 16.0, 3.0, &col::css(group_color), true);
    let mut x = 26.0;
    x += p.text(x, 9.0, name, &f1, &col::css(col::PAPER_100));
    x += p.text(x, 9.0, " · ", &f1, &col::css(col::FG_FAINT));
    p.text(x, 9.0, kind, &f1, &col::css(col::FG_MUTED));
    if let Some(l) = &lines.1 {
        p.text(26.0, 28.0, l, &f2, &col::css(col::FG_FAINT));
    }
    p.dirty = true;
    const M_PER_PX: f32 = 0.0017;
    (w as f32 * M_PER_PX, h as f32 * M_PER_PX)
}
/// Distance along a ray to a sphere, if it is hit ahead.
fn ray_sphere(origin: Vec3, dir: Vec3, c: Vec3, r: f32) -> Option<f32> {
    let oc = origin - c;
    let b = oc.dot(dir);
    let cc = oc.dot(oc) - r * r;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t > 0.0 {
        Some(t)
    } else if -b + disc.sqrt() > 0.0 {
        Some(0.0)
    } else {
        None
    }
}
