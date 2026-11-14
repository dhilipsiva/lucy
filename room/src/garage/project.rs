//! Pictures onto surfaces (roadmap/N8j.md). Each plane and mesh of a captured home is cut into
//! cells; each cell takes the picture that sees it most squarely and closely, and its corners
//! are projected through that picture's camera for texture coordinates. Nothing here tests what
//! stands between a camera and a surface, so a picture can paint a wall through furniture.

use crate::geom::Mesh;
use glam::{Mat4, Quat, Vec2, Vec3};
use lucy_room_memory::home::{Alignment, HomeCapture, PathPoint, Photo};

/// The side of a plane's cell, in metres.
pub const CELL: f32 = 0.25;
/// A picture must see a cell at least this squarely (cosine of the angle off its normal).
pub const MIN_FACING: f32 = 0.25;
/// Nearer than this the camera is inside the furniture.
pub const NEAR: f32 = 0.15;

pub fn mat(m: &[f32; 16]) -> Mat4 {
    Mat4::from_cols_array(m)
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub world: Mat4,
    inverse: Mat4,
    tan_h: f32,
    tan_v: f32,
}
impl Camera {
    /// The camera for a head pose: the alignment's offset and turn against the head, its
    /// horizontal field of view, and the picture's width over height.
    pub fn new(head: Mat4, a: &Alignment, aspect: f32) -> Camera {
        let turn = Quat::from_rotation_y(a.yaw_deg.to_radians())
            * Quat::from_rotation_x(a.pitch_deg.to_radians())
            * Quat::from_rotation_z(a.roll_deg.to_radians());
        let world = head * Mat4::from_rotation_translation(turn, Vec3::new(a.dx, a.dy, a.dz));
        let tan_h = (a.hfov_deg.to_radians() / 2.0).tan();
        Camera {
            world,
            inverse: world.inverse(),
            tan_h,
            tan_v: tan_h / aspect.max(0.01),
        }
    }
    pub fn position(&self) -> Vec3 {
        self.world.w_axis.truncate()
    }
    /// Where a point of the world lands in the picture: (0,0) top left, (1,1) bottom right.
    /// None behind the camera, too near, or outside the frame.
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.inverse.transform_point3(p);
        let depth = -c.z;
        if depth < NEAR {
            return None;
        }
        let x = c.x / (depth * self.tan_h);
        let y = c.y / (depth * self.tan_v);
        if x.abs() > 1.0 || y.abs() > 1.0 {
            return None;
        }
        Some(Vec2::new((x + 1.0) / 2.0, (1.0 - y) / 2.0))
    }
}

/// The head's pose for a picture: as recorded, or, when the picture trails its pose by
/// `lag_s`, read back along the walked path.
pub fn head_pose(photo: &Photo, path: &[PathPoint], lag_s: f32) -> Mat4 {
    if lag_s.abs() < 1e-3 || path.len() < 2 {
        return mat(&photo.pose);
    }
    let t = photo.t - lag_s;
    let i = path.partition_point(|p| p.t <= t);
    let (a, b) = match i {
        0 => (path[0], path[0]),
        n if n >= path.len() => (path[path.len() - 1], path[path.len() - 1]),
        n => (path[n - 1], path[n]),
    };
    let k = if b.t > a.t {
        ((t - a.t) / (b.t - a.t)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let pos = Vec3::from(a.pos).lerp(Vec3::from(b.pos), k);
    let rot = Quat::from_array(a.quat)
        .normalize()
        .slerp(Quat::from_array(b.quat).normalize(), k);
    Mat4::from_rotation_translation(rot, pos)
}

pub fn cameras(c: &HomeCapture, a: &Alignment) -> Vec<Camera> {
    c.photos
        .iter()
        .map(|p| {
            Camera::new(
                head_pose(p, &c.path, a.lag_s),
                a,
                p.width as f32 / p.height.max(1) as f32,
            )
        })
        .collect()
}

/// One flat piece of a surface: three or four corners in the world, and its normal.
#[derive(Clone, Debug)]
pub struct Cell {
    pub corners: Vec<Vec3>,
    pub normal: Vec3,
}
impl Cell {
    pub fn centre(&self) -> Vec3 {
        self.corners.iter().copied().sum::<Vec3>() / self.corners.len() as f32
    }
}

fn inside(poly: &[Vec2], p: Vec2) -> bool {
    let mut hit = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// Every surface of the capture as cells: planes as a grid over their outline, meshes as
/// their own triangles.
pub fn cells(c: &HomeCapture) -> Vec<Cell> {
    let mut out = vec![];
    for plane in &c.planes {
        if plane.polygon.len() < 3 {
            continue;
        }
        let pose = mat(&plane.pose);
        let poly: Vec<Vec2> = plane
            .polygon
            .iter()
            .map(|p| Vec2::new(p[0], p[2]))
            .collect();
        let lo = poly.iter().copied().fold(Vec2::splat(f32::MAX), Vec2::min);
        let hi = poly.iter().copied().fold(Vec2::splat(f32::MIN), Vec2::max);
        let size = hi - lo;
        let (nx, nz) = (
            (size.x / CELL).ceil().clamp(1.0, 400.0) as usize,
            (size.y / CELL).ceil().clamp(1.0, 400.0) as usize,
        );
        let step = Vec2::new(size.x / nx as f32, size.y / nz as f32);
        let normal = pose.transform_vector3(Vec3::Y).normalize_or_zero();
        for iz in 0..nz {
            for ix in 0..nx {
                let a = lo + Vec2::new(ix as f32 * step.x, iz as f32 * step.y);
                let b = a + step;
                if !inside(&poly, (a + b) / 2.0) {
                    continue;
                }
                let corner = |x: f32, z: f32| pose.transform_point3(Vec3::new(x, 0.0, z));
                out.push(Cell {
                    corners: vec![
                        corner(a.x, a.y),
                        corner(b.x, a.y),
                        corner(b.x, b.y),
                        corner(a.x, b.y),
                    ],
                    normal,
                });
            }
        }
    }
    for mesh in &c.meshes {
        let pose = mat(&mesh.pose);
        let v = |i: u32| {
            let i = i as usize * 3;
            pose.transform_point3(Vec3::new(
                mesh.vertices[i],
                mesh.vertices[i + 1],
                mesh.vertices[i + 2],
            ))
        };
        for t in mesh.indices.chunks(3).filter(|t| t.len() == 3) {
            let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
            let normal = (b - a).cross(c - a).normalize_or_zero();
            if normal != Vec3::ZERO {
                out.push(Cell {
                    corners: vec![a, b, c],
                    normal,
                });
            }
        }
    }
    out
}

/// For each cell, the picture that shows all of it most squarely and closely, if any does.
pub fn assign(cells: &[Cell], cams: &[Camera]) -> Vec<Option<usize>> {
    cells
        .iter()
        .map(|cell| {
            let centre = cell.centre();
            let mut best: Option<(f32, usize)> = None;
            for (i, cam) in cams.iter().enumerate() {
                let to = cam.position() - centre;
                let dist = to.length();
                if dist < NEAR {
                    continue;
                }
                // either face of a surface may be the one the room sees
                let facing = (cell.normal.dot(to) / dist).abs();
                if facing < MIN_FACING {
                    continue;
                }
                let score = facing / (1.0 + dist);
                if best.is_some_and(|(s, _)| s >= score) {
                    continue;
                }
                if cell.corners.iter().all(|p| cam.project(*p).is_some()) {
                    best = Some((score, i));
                }
            }
            best.map(|(_, i)| i)
        })
        .collect()
}

fn push(m: &mut Mesh, p: Vec3, n: Vec3, uv: Vec2) {
    m.positions.extend_from_slice(&[p.x, p.y, p.z]);
    m.normals.extend_from_slice(&[n.x, n.y, n.z]);
    m.uvs.extend_from_slice(&[uv.x, uv.y]);
}

/// What the viewer draws: the cells no picture shows, and one mesh per picture used.
pub struct Painted {
    pub bare: Mesh,
    pub pictures: Vec<(usize, Mesh)>,
    pub cells: usize,
    pub textured: usize,
}

pub fn paint(c: &HomeCapture, a: &Alignment) -> Painted {
    let cells = cells(c);
    let cams = cameras(c, a);
    let chosen = assign(&cells, &cams);
    let mut bare = Mesh::default();
    let mut by_picture: std::collections::BTreeMap<usize, Mesh> = Default::default();
    let mut textured = 0;
    for (cell, pick) in cells.iter().zip(&chosen) {
        let (mesh, uv): (&mut Mesh, Vec<Vec2>) = match pick {
            Some(i) => {
                textured += 1;
                (
                    by_picture.entry(*i).or_default(),
                    cell.corners
                        .iter()
                        .map(|p| cams[*i].project(*p).unwrap_or(Vec2::ZERO))
                        .collect(),
                )
            }
            None => (&mut bare, vec![Vec2::ZERO; cell.corners.len()]),
        };
        let base = (mesh.positions.len() / 3) as u32;
        for (p, uv) in cell.corners.iter().zip(&uv) {
            push(mesh, *p, cell.normal, *uv);
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        if cell.corners.len() == 4 {
            mesh.indices.extend_from_slice(&[base, base + 2, base + 3]);
        }
    }
    Painted {
        bare,
        pictures: by_picture.into_iter().collect(),
        cells: cells.len(),
        textured,
    }
}

/// What a surface is, for a home drawn without its pictures (roadmap/N8j.v3.md): each kind
/// gets a colour of its own so the rooms read as rooms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Wall,
    Floor,
    Ceiling,
    /// Any other labelled plane: a shelf, a table, a couch, a door.
    Furniture,
    /// The headset's unlabelled mesh of the room.
    Shape,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Wall,
        Kind::Floor,
        Kind::Ceiling,
        Kind::Furniture,
        Kind::Shape,
    ];
    pub fn of(label: &str) -> Kind {
        match label {
            "wall" => Kind::Wall,
            "floor" => Kind::Floor,
            "ceiling" => Kind::Ceiling,
            _ => Kind::Furniture,
        }
    }
    /// sRGB, as `colors::linear` takes it.
    pub fn color(self) -> u32 {
        match self {
            Kind::Wall => 0xE9E4DA,
            Kind::Floor => 0x8B6F52,
            Kind::Ceiling => 0xF5F3EE,
            Kind::Furniture => 0x4F7CAC,
            Kind::Shape => 0xB8B2A6,
        }
    }
}

/// The middle of the labelled surfaces, for a first look that faces the home.
pub fn middle(c: &HomeCapture) -> Option<Vec3> {
    let centres: Vec<Vec3> = cells(&HomeCapture {
        meshes: vec![],
        path: vec![],
        photos: vec![],
        ..c.clone()
    })
    .iter()
    .map(Cell::centre)
    .collect();
    (!centres.is_empty()).then(|| centres.iter().copied().sum::<Vec3>() / centres.len() as f32)
}

/// The home's surfaces with no picture on them: one mesh per kind that has any surface.
pub fn plain(c: &HomeCapture) -> Vec<(Kind, Mesh)> {
    Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let only = HomeCapture {
                planes: c
                    .planes
                    .iter()
                    .filter(|p| kind != Kind::Shape && Kind::of(&p.label) == kind)
                    .cloned()
                    .collect(),
                meshes: if kind == Kind::Shape {
                    c.meshes.clone()
                } else {
                    vec![]
                },
                path: vec![],
                photos: vec![],
                ..c.clone()
            };
            let mut mesh = Mesh::default();
            for cell in cells(&only) {
                let base = (mesh.positions.len() / 3) as u32;
                // The normal follows the way the corners wind, not the plane's own axis: the
                // shader turns it for a face seen from behind, and on his Quest the floor's
                // axis points down, which lit it as if from underneath.
                let wound = (cell.corners[1] - cell.corners[0])
                    .cross(cell.corners[2] - cell.corners[0])
                    .normalize_or_zero();
                let normal = if wound == Vec3::ZERO {
                    cell.normal
                } else {
                    wound
                };
                for p in &cell.corners {
                    push(&mut mesh, *p, normal, Vec2::ZERO);
                }
                mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
                if cell.corners.len() == 4 {
                    mesh.indices.extend_from_slice(&[base, base + 2, base + 3]);
                }
            }
            (!mesh.indices.is_empty()).then_some((kind, mesh))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lucy_room_memory::home::{Detector, Plane, Stats, SurfaceMesh, FORMAT};

    #[test]
    fn a_plain_home_has_one_mesh_for_each_kind_of_surface() {
        let mut c = capture(vec![]);
        let mut floor = wall();
        floor.label = "floor".into();
        let mut shelf = wall();
        shelf.label = "shelf".into();
        c.planes = vec![wall(), wall(), floor, shelf];
        c.meshes = vec![SurfaceMesh {
            label: String::new(),
            pose: Mat4::IDENTITY.to_cols_array(),
            vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            indices: vec![0, 1, 2],
        }];
        let p = plain(&c);
        let kinds: Vec<Kind> = p.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            [Kind::Wall, Kind::Floor, Kind::Furniture, Kind::Shape]
        );
        // two walls of 8x8 cells, two triangles each
        assert_eq!(p[0].1.indices.len(), 2 * 64 * 6);
        assert_eq!(p[3].1.indices.len(), 3);
        // every surface of the capture is in exactly one of them
        let all: usize = p.iter().map(|(_, m)| m.indices.len()).sum();
        let whole = paint(&c, &straight());
        assert_eq!(all, whole.bare.indices.len());
    }

    fn straight() -> Alignment {
        Alignment {
            hfov_deg: 90.0,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            roll_deg: 0.0,
            dx: 0.0,
            dy: 0.0,
            dz: 0.0,
            lag_s: 0.0,
        }
    }
    fn photo(id: u32, t: f32, pos: Vec3) -> Photo {
        Photo {
            id,
            t,
            pose: Mat4::from_translation(pos).to_cols_array(),
            width: 640,
            height: 480,
            faces_blurred: 0,
            jpeg_base64: String::new(),
        }
    }
    /// A 2 m square wall 2 m ahead (z = −2), facing the viewer, centred at eye height.
    fn wall() -> Plane {
        Plane {
            label: "wall".into(),
            orientation: "vertical".into(),
            pose: Mat4::from_rotation_translation(
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                Vec3::new(0.0, 1.6, -2.0),
            )
            .to_cols_array(),
            polygon: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
                [-1.0, 0.0, 1.0],
            ],
        }
    }
    fn capture(photos: Vec<Photo>) -> HomeCapture {
        HomeCapture {
            format: FORMAT.into(),
            id: "t".into(),
            started: String::new(),
            ended: String::new(),
            device: String::new(),
            reference_space: "local-floor".into(),
            raw: false,
            planes: vec![wall()],
            meshes: vec![],
            path: vec![],
            photos,
            stats: Stats::default(),
            detector: Detector::default(),
            alignment: straight(),
            notes: vec![],
        }
    }

    #[test]
    fn straight_ahead_is_the_middle_of_the_picture() {
        let cam = Camera::new(
            Mat4::from_translation(Vec3::new(0.0, 1.6, 0.0)),
            &straight(),
            4.0 / 3.0,
        );
        let mid = cam.project(Vec3::new(0.0, 1.6, -2.0)).unwrap();
        assert!((mid - Vec2::splat(0.5)).length() < 1e-5);
        let right_up = cam.project(Vec3::new(0.5, 2.0, -2.0)).unwrap();
        assert!(right_up.x > 0.5 && right_up.y < 0.5, "{right_up}");
        // 90° across: the edge of the frame is one metre aside at one metre ahead
        let edge = cam.project(Vec3::new(0.999, 1.6, -1.0)).unwrap();
        assert!(edge.x > 0.99);
        assert!(cam.project(Vec3::new(1.2, 1.6, -1.0)).is_none());
        assert!(cam.project(Vec3::new(0.0, 1.6, 2.0)).is_none(), "behind");
        assert!(
            cam.project(Vec3::new(0.0, 1.6, -0.05)).is_none(),
            "too near"
        );
    }

    #[test]
    fn the_offset_and_turn_move_the_picture() {
        let mut a = straight();
        a.yaw_deg = 10.0; // the camera looks a little left of the head
        let cam = Camera::new(
            Mat4::from_translation(Vec3::new(0.0, 1.6, 0.0)),
            &a,
            4.0 / 3.0,
        );
        let p = cam.project(Vec3::new(0.0, 1.6, -2.0)).unwrap();
        assert!(p.x > 0.5, "straight ahead now sits right of centre: {p}");
    }

    #[test]
    fn a_wall_becomes_a_grid_and_takes_the_nearer_squarer_picture() {
        let c = capture(vec![
            photo(0, 0.0, Vec3::new(0.0, 1.6, 3.0)),  // five metres back
            photo(1, 1.0, Vec3::new(0.0, 1.6, 0.0)),  // two metres back
            photo(2, 2.0, Vec3::new(0.0, 1.6, -4.0)), // behind the wall, looking away
        ]);
        let cells = cells(&c);
        assert_eq!(cells.len(), 64);
        for cell in &cells {
            assert!((cell.centre().z + 2.0).abs() < 1e-4);
            assert!(cell.normal.z.abs() > 0.99);
        }
        let painted = paint(&c, &straight());
        assert_eq!(painted.cells, 64);
        // at 90° across and two metres back, the middle of the wall is in frame; its corners
        // (a metre aside and a metre up, on a 4:3 picture) are not, and fall to the far one
        let near: usize = painted
            .pictures
            .iter()
            .filter(|(i, _)| *i == 1)
            .map(|(_, m)| m.indices.len() / 6)
            .sum();
        let far: usize = painted
            .pictures
            .iter()
            .filter(|(i, _)| *i == 0)
            .map(|(_, m)| m.indices.len() / 6)
            .sum();
        assert!(near >= 30, "{near}");
        assert_eq!(near + far, 64);
        assert!(painted.pictures.iter().all(|(i, _)| *i != 2));
        assert!(painted.bare.indices.is_empty());
        for (_, m) in &painted.pictures {
            assert_eq!(m.uvs.len() / 2, m.positions.len() / 3);
            assert!(m.uvs.iter().all(|u| (0.0..=1.0).contains(u)));
        }
    }

    #[test]
    fn nothing_seen_stays_bare_and_meshes_are_their_triangles() {
        let mut c = capture(vec![]);
        c.meshes.push(SurfaceMesh {
            label: "global mesh".into(),
            pose: Mat4::IDENTITY.to_cols_array(),
            vertices: vec![
                0.0, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, -1.0, 5.0, 5.0, 5.0,
            ],
            indices: vec![0, 1, 2],
        });
        let painted = paint(&c, &straight());
        assert_eq!(painted.cells, 65);
        assert_eq!(painted.textured, 0);
        assert_eq!(painted.bare.indices.len(), 64 * 6 + 3);
    }

    #[test]
    fn a_lagging_picture_is_read_back_along_the_path() {
        let path = vec![
            PathPoint {
                t: 0.0,
                pos: [0.0, 1.6, 0.0],
                quat: [0.0, 0.0, 0.0, 1.0],
            },
            PathPoint {
                t: 1.0,
                pos: [1.0, 1.6, 0.0],
                quat: [0.0, 0.0, 0.0, 1.0],
            },
        ];
        let p = photo(0, 1.0, Vec3::new(1.0, 1.6, 0.0));
        assert_eq!(head_pose(&p, &path, 0.0).w_axis.x, 1.0);
        let lagged = head_pose(&p, &path, 0.5);
        assert!((lagged.w_axis.x - 0.5).abs() < 1e-5);
        assert_eq!(
            head_pose(&p, &path, 9.0).w_axis.x,
            0.0,
            "clamped to the start"
        );
    }
}
