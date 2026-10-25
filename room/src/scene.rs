//! The scene graph: nodes with a local transform and a thing to draw (a lit mesh, lines, points,
//! a billboard sprite, a textured board, a point light), rendered opaque first and transparent
//! back to front, as three.js did for the hand-off.

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::gl::{Blend, Gl, GpuMesh, Lights, LitParams, PointLight, Texture};

pub type NodeId = usize;

#[derive(Clone, Debug)]
pub struct Material {
    pub color: [f32; 3],
    pub emissive: [f32; 3],
    pub emissive_intensity: f32,
    pub roughness: f32,
    pub opacity: f32,
    pub transparent: bool,
    pub double_sided: bool,
    pub depth_write: bool,
    pub map: Option<usize>,
}
impl Material {
    /// three.js `std(color, {glow, rough, alpha})`: emissive is the colour itself unless given.
    pub fn std(color: u32) -> Material {
        let c = crate::colors::linear(color);
        Material {
            color: c,
            emissive: c,
            emissive_intensity: 0.0,
            roughness: 0.6,
            opacity: 1.0,
            transparent: false,
            double_sided: false,
            depth_write: true,
            map: None,
        }
    }
    pub fn glow(mut self, g: f32) -> Material {
        self.emissive_intensity = g;
        self
    }
    pub fn rough(mut self, r: f32) -> Material {
        self.roughness = r;
        self
    }
    pub fn emissive(mut self, hex: u32) -> Material {
        self.emissive = crate::colors::linear(hex);
        self
    }
    pub fn alpha(mut self, a: f32) -> Material {
        self.opacity = a;
        self.transparent = true;
        self
    }
    pub fn two_sided(mut self) -> Material {
        self.double_sided = true;
        self
    }
    pub fn no_depth_write(mut self) -> Material {
        self.depth_write = false;
        self
    }
    pub fn with_map(mut self, t: usize) -> Material {
        self.map = Some(t);
        self
    }
}

pub enum Kind {
    Empty,
    Mesh {
        mesh: usize,
        material: Material,
    },
    Lines {
        mesh: usize,
        color: [f32; 3],
        opacity: f32,
    },
    Points {
        mesh: usize,
        color: [f32; 3],
        size: f32,
        opacity: f32,
    },
    /// A camera-facing quad with a texture, additive when `additive` (the halos).
    Sprite {
        texture: usize,
        color: [f32; 3],
        opacity: f32,
        additive: bool,
    },
    /// A textured quad in the world (the boards), premultiplied alpha, `w` by `h` metres.
    Board {
        texture: usize,
        w: f32,
        h: f32,
        opacity: f32,
    },
    Light {
        color: [f32; 3],
        intensity: f32,
        distance: f32,
        decay: f32,
    },
}

pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub visible: bool,
    pub kind: Kind,
    pub world: Mat4,
}
impl Node {
    pub fn new(kind: Kind) -> Node {
        Node {
            parent: None,
            children: Vec::new(),
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            visible: true,
            kind,
            world: Mat4::IDENTITY,
        }
    }
}
/// three.js Euler XYZ, intrinsic.
pub fn euler(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_euler(EulerRot::XYZ, x, y, z)
}

pub struct Scene {
    pub nodes: Vec<Node>,
    pub meshes: Vec<GpuMesh>,
    pub textures: Vec<Texture>,
    pub lights: Lights,
    pub root: NodeId,
    quad: usize,
}
impl Scene {
    pub fn new(gl: &Gl, lights: Lights) -> Scene {
        let quad = gl.upload(&crate::geom::plane(1.0, 1.0));
        let mut s = Scene {
            nodes: Vec::new(),
            meshes: vec![quad],
            textures: Vec::new(),
            lights,
            root: 0,
            quad: 0,
        };
        s.nodes.push(Node::new(Kind::Empty));
        s
    }
    pub fn add_mesh(&mut self, m: GpuMesh) -> usize {
        self.meshes.push(m);
        self.meshes.len() - 1
    }
    pub fn add_texture(&mut self, t: Texture) -> usize {
        self.textures.push(t);
        self.textures.len() - 1
    }
    pub fn add(&mut self, parent: NodeId, mut node: Node) -> NodeId {
        node.parent = Some(parent);
        let id = self.nodes.len();
        self.nodes.push(node);
        self.nodes[parent].children.push(id);
        id
    }
    pub fn node(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }
    pub fn set_pos(&mut self, id: NodeId, p: Vec3) {
        self.nodes[id].position = p;
    }
    pub fn set_rot(&mut self, id: NodeId, x: f32, y: f32, z: f32) {
        self.nodes[id].rotation = euler(x, y, z);
    }
    pub fn set_scale(&mut self, id: NodeId, s: Vec3) {
        self.nodes[id].scale = s;
    }
    pub fn set_visible(&mut self, id: NodeId, v: bool) {
        self.nodes[id].visible = v;
    }
    pub fn material(&mut self, id: NodeId) -> Option<&mut Material> {
        match &mut self.nodes[id].kind {
            Kind::Mesh { material, .. } => Some(material),
            _ => None,
        }
    }
    pub fn set_opacity(&mut self, id: NodeId, o: f32) {
        match &mut self.nodes[id].kind {
            Kind::Mesh { material, .. } => {
                material.opacity = o;
                material.transparent = o < 1.0;
            }
            Kind::Lines { opacity, .. }
            | Kind::Points { opacity, .. }
            | Kind::Sprite { opacity, .. }
            | Kind::Board { opacity, .. } => *opacity = o,
            _ => {}
        }
    }
    /// Recompute every world matrix, parents first.
    pub fn update_world(&mut self) {
        let mut stack = vec![self.root];
        self.nodes[self.root].world = Mat4::IDENTITY;
        while let Some(id) = stack.pop() {
            let parent_world = self.nodes[id]
                .parent
                .map(|p| self.nodes[p].world)
                .unwrap_or(Mat4::IDENTITY);
            let n = &mut self.nodes[id];
            n.world = parent_world
                * Mat4::from_scale_rotation_translation(n.scale, n.rotation, n.position);
            stack.extend(n.children.iter().copied());
        }
    }
    pub fn world_position(&self, id: NodeId) -> Vec3 {
        self.nodes[id].world.transform_point3(Vec3::ZERO)
    }
    /// Is this node drawn, given its ancestors' visibility?
    fn shown(&self, mut id: NodeId) -> bool {
        loop {
            if !self.nodes[id].visible {
                return false;
            }
            match self.nodes[id].parent {
                Some(p) => id = p,
                None => return true,
            }
        }
    }
    /// Draw the scene: lights gathered, opaque meshes, then transparent things far to near.
    pub fn render(&mut self, gl: &Gl, view: &Mat4, proj: &Mat4, cam_pos: Vec3) {
        self.update_world();
        // gather point lights
        let mut points = Vec::new();
        for (id, n) in self.nodes.iter().enumerate() {
            if let Kind::Light {
                color,
                intensity,
                distance,
                decay,
            } = n.kind
            {
                if self.shown(id) && intensity > 0.0 {
                    points.push(PointLight {
                        pos: n.world.transform_point3(Vec3::ZERO),
                        color,
                        intensity,
                        distance,
                        decay,
                    });
                }
            }
        }
        self.lights.points = points;
        let mut transparent: Vec<(f32, NodeId)> = Vec::new();
        for id in 0..self.nodes.len() {
            if !self.shown(id) {
                continue;
            }
            let n = &self.nodes[id];
            match &n.kind {
                Kind::Mesh { mesh, material }
                    if !material.transparent && material.opacity >= 1.0 =>
                {
                    gl.draw_lit(
                        &self.meshes[*mesh],
                        &lit_params(n.world, material, &self.textures),
                        view,
                        proj,
                        &self.lights,
                        cam_pos,
                    );
                }
                Kind::Lines {
                    mesh,
                    color,
                    opacity,
                } => gl.draw_lines(
                    &self.meshes[*mesh],
                    &n.world,
                    view,
                    proj,
                    *color,
                    *opacity,
                    &self.lights,
                ),
                Kind::Points {
                    mesh,
                    color,
                    size,
                    opacity,
                } => gl.draw_points(
                    &self.meshes[*mesh],
                    &n.world,
                    view,
                    proj,
                    *color,
                    *size,
                    *opacity,
                    &self.lights,
                ),
                Kind::Empty | Kind::Light { .. } => {}
                _ => {
                    let d = view
                        .transform_point3(n.world.transform_point3(Vec3::ZERO))
                        .z;
                    transparent.push((d, id));
                }
            }
        }
        transparent.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); // most negative z = farthest first
        for (_, id) in transparent {
            let n = &self.nodes[id];
            match &n.kind {
                Kind::Mesh { mesh, material } => gl.draw_lit(
                    &self.meshes[*mesh],
                    &lit_params(n.world, material, &self.textures),
                    view,
                    proj,
                    &self.lights,
                    cam_pos,
                ),
                Kind::Sprite {
                    texture,
                    color,
                    opacity,
                    additive,
                } => {
                    // billboard: the camera's rotation, the node's world position and scale
                    let pos = n.world.transform_point3(Vec3::ZERO);
                    let scale = n.world.transform_vector3(Vec3::X).length();
                    let cam_rot = Quat::from_mat4(&view.inverse());
                    let model =
                        Mat4::from_scale_rotation_translation(Vec3::splat(scale), cam_rot, pos);
                    let mvp = *proj * *view * model;
                    gl.draw_flat(
                        &self.meshes[self.quad],
                        &mvp,
                        Some(&self.textures[*texture]),
                        [
                            color[0] * *opacity,
                            color[1] * *opacity,
                            color[2] * *opacity,
                            *opacity,
                        ],
                        if *additive {
                            Blend::Additive
                        } else {
                            Blend::Premultiplied
                        },
                        true,
                        false,
                    );
                }
                Kind::Board {
                    texture,
                    w,
                    h,
                    opacity,
                } => {
                    let model = n.world * Mat4::from_scale(Vec3::new(*w, *h, 1.0));
                    let mvp = *proj * *view * model;
                    gl.draw_flat(
                        &self.meshes[self.quad],
                        &mvp,
                        Some(&self.textures[*texture]),
                        [1.0, 1.0, 1.0, *opacity],
                        Blend::Premultiplied,
                        true,
                        true,
                    );
                }
                _ => {}
            }
        }
    }
    pub fn quad_mesh(&self) -> usize {
        self.quad
    }
}
fn lit_params<'a>(model: Mat4, m: &Material, textures: &'a [Texture]) -> LitParams<'a> {
    LitParams {
        model,
        color: m.color,
        emissive: m.emissive,
        emissive_intensity: m.emissive_intensity,
        roughness: m.roughness,
        opacity: m.opacity,
        map: m.map.map(|i| &textures[i]),
        double_sided: m.double_sided,
        depth_write: m.depth_write,
    }
}
