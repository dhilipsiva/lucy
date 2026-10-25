//! Procedural meshes for the room, the shapes three.js gave the hand-off, built here in Rust:
//! spheres, capsules, cylinders and cones, boxes, tori, rings, planes, and fins from a spline
//! outline. Positions, normals and texture coordinates, indexed triangles; lines for grids and
//! edges. Pure math: the native tests check them.

use glam::{Vec2, Vec3};

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u32>,
}
impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    fn push(&mut self, p: Vec3, n: Vec3, uv: Vec2) -> u32 {
        let i = self.vertex_count() as u32;
        self.positions.extend_from_slice(&[p.x, p.y, p.z]);
        self.normals.extend_from_slice(&[n.x, n.y, n.z]);
        self.uvs.extend_from_slice(&[uv.x, uv.y]);
        i
    }
    /// Merge another mesh in, offsetting its indices.
    pub fn append(&mut self, other: &Mesh) {
        let base = self.vertex_count() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
    /// Move every vertex (three.js `geometry.translate`).
    pub fn translate(mut self, d: Vec3) -> Self {
        for p in self.positions.chunks_mut(3) {
            p[0] += d.x;
            p[1] += d.y;
            p[2] += d.z;
        }
        self
    }
}

/// A UV sphere (three.js SphereGeometry(radius, widthSegments, heightSegments)).
pub fn sphere(radius: f32, width: u32, height: u32) -> Mesh {
    let mut m = Mesh::default();
    let (w, h) = (width.max(3), height.max(2));
    for iy in 0..=h {
        let v = iy as f32 / h as f32;
        let theta = v * std::f32::consts::PI;
        for ix in 0..=w {
            let u = ix as f32 / w as f32;
            let phi = u * std::f32::consts::TAU;
            let n = Vec3::new(
                -theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
            m.push(n * radius, n, Vec2::new(u, 1.0 - v));
        }
    }
    for iy in 0..h {
        for ix in 0..w {
            let a = iy * (w + 1) + ix + 1;
            let b = iy * (w + 1) + ix;
            let c = (iy + 1) * (w + 1) + ix;
            let d = (iy + 1) * (w + 1) + ix + 1;
            if iy != 0 {
                m.indices.extend_from_slice(&[a, b, d]);
            }
            if iy != h - 1 {
                m.indices.extend_from_slice(&[b, c, d]);
            }
        }
    }
    m
}

/// Revolve a profile (radius, y) about the Y axis; the profile runs bottom to top.
pub fn lathe(profile: &[(f32, f32)], segments: u32) -> Mesh {
    let mut m = Mesh::default();
    let seg = segments.max(3);
    // normals from the profile's tangent
    let n = profile.len();
    for (i, &(r, y)) in profile.iter().enumerate() {
        let (r0, y0) = if i == 0 { profile[0] } else { profile[i - 1] };
        let (r1, y1) = if i + 1 == n {
            profile[n - 1]
        } else {
            profile[i + 1]
        };
        let t = Vec2::new(r1 - r0, y1 - y0);
        let nn = if t.length_squared() < 1e-12 {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(t.y, -t.x).normalize()
        };
        for s in 0..=seg {
            let a = s as f32 / seg as f32 * std::f32::consts::TAU;
            let (sa, ca) = a.sin_cos();
            m.push(
                Vec3::new(r * sa, y, r * ca),
                Vec3::new(nn.x * sa, nn.y, nn.x * ca).normalize_or_zero(),
                Vec2::new(s as f32 / seg as f32, i as f32 / (n - 1).max(1) as f32),
            );
        }
    }
    for i in 0..(n as u32 - 1) {
        for s in 0..seg {
            let a = i * (seg + 1) + s;
            let b = a + seg + 1;
            m.indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    m
}

/// A capsule along Y (three.js CapsuleGeometry(radius, length, capSegments, radialSegments)): a
/// cylinder of `length` with hemispherical caps, centred at the origin.
pub fn capsule(radius: f32, length: f32, cap_segments: u32, radial: u32) -> Mesh {
    let mut profile = Vec::new();
    let caps = cap_segments.max(2);
    for i in 0..=caps {
        let a =
            -std::f32::consts::FRAC_PI_2 + (i as f32 / caps as f32) * std::f32::consts::FRAC_PI_2;
        profile.push((radius * a.cos(), -length / 2.0 + radius * a.sin()));
    }
    for i in 0..=caps {
        let a = (i as f32 / caps as f32) * std::f32::consts::FRAC_PI_2;
        profile.push((radius * a.cos(), length / 2.0 + radius * a.sin()));
    }
    lathe(&profile, radial)
}

/// A cylinder or cone along Y with flat caps (three.js CylinderGeometry(rTop, rBottom, height, radialSegments)).
pub fn cylinder(r_top: f32, r_bottom: f32, height: f32, radial: u32) -> Mesh {
    let seg = radial.max(3);
    let mut m = Mesh::default();
    let hh = height / 2.0;
    // side
    let slope = (r_bottom - r_top) / height;
    for (i, (r, y)) in [(r_bottom, -hh), (r_top, hh)].iter().enumerate() {
        for s in 0..=seg {
            let a = s as f32 / seg as f32 * std::f32::consts::TAU;
            let (sa, ca) = a.sin_cos();
            m.push(
                Vec3::new(r * sa, *y, r * ca),
                Vec3::new(sa, slope, ca).normalize(),
                Vec2::new(s as f32 / seg as f32, i as f32),
            );
        }
    }
    for s in 0..seg {
        let a = s;
        let b = s + seg + 1;
        m.indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
    }
    // caps
    for (r, y, up) in [(r_top, hh, true), (r_bottom, -hh, false)] {
        if r <= 0.0 {
            continue;
        }
        let n = Vec3::new(0.0, if up { 1.0 } else { -1.0 }, 0.0);
        let c = m.push(Vec3::new(0.0, y, 0.0), n, Vec2::new(0.5, 0.5));
        let first = m.vertex_count() as u32;
        for s in 0..=seg {
            let a = s as f32 / seg as f32 * std::f32::consts::TAU;
            let (sa, ca) = a.sin_cos();
            m.push(
                Vec3::new(r * sa, y, r * ca),
                n,
                Vec2::new(0.5 + 0.5 * sa, 0.5 + 0.5 * ca),
            );
        }
        for s in 0..seg {
            if up {
                m.indices.extend_from_slice(&[c, first + s, first + s + 1]);
            } else {
                m.indices.extend_from_slice(&[c, first + s + 1, first + s]);
            }
        }
    }
    m
}
pub fn cone(radius: f32, height: f32, radial: u32) -> Mesh {
    cylinder(0.0, radius, height, radial)
}

/// A box centred at the origin.
pub fn cuboid(w: f32, h: f32, d: f32) -> Mesh {
    let mut m = Mesh::default();
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Z, Vec3::Y),
        (-Vec3::X, -Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, -Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::X, Vec3::Y),
    ];
    let half = Vec3::new(x, y, z);
    for (n, u, v) in faces {
        let base = m.vertex_count() as u32;
        for (su, sv, uv) in [
            (-1.0, -1.0, (0.0, 0.0)),
            (1.0, -1.0, (1.0, 0.0)),
            (1.0, 1.0, (1.0, 1.0)),
            (-1.0, 1.0, (0.0, 1.0)),
        ] {
            let p = (n + u * su + v * sv) * half;
            m.push(p, n, Vec2::new(uv.0, uv.1));
        }
        m.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}
/// The twelve edges of a box, as line segments.
pub fn cuboid_edges(w: f32, h: f32, d: f32) -> Vec<f32> {
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    let c = |sx: f32, sy: f32, sz: f32| [sx * x, sy * y, sz * z];
    let mut v = Vec::new();
    let mut seg = |a: [f32; 3], b: [f32; 3]| {
        v.extend_from_slice(&a);
        v.extend_from_slice(&b);
    };
    for sy in [-1.0, 1.0] {
        seg(c(-1.0, sy, -1.0), c(1.0, sy, -1.0));
        seg(c(1.0, sy, -1.0), c(1.0, sy, 1.0));
        seg(c(1.0, sy, 1.0), c(-1.0, sy, 1.0));
        seg(c(-1.0, sy, 1.0), c(-1.0, sy, -1.0));
    }
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            seg(c(sx, -1.0, sz), c(sx, 1.0, sz));
        }
    }
    v
}

/// Every unique edge of an indexed mesh as line segments (three.js WireframeGeometry).
pub fn wire_lines(m: &Mesh) -> Vec<f32> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for tri in m.indices.chunks(3) {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            if seen.insert(key) {
                out.extend_from_slice(&m.positions[a as usize * 3..a as usize * 3 + 3]);
                out.extend_from_slice(&m.positions[b as usize * 3..b as usize * 3 + 3]);
            }
        }
    }
    out
}

/// A torus about Y... three.js builds it about Z (the ring lies in the XY plane); so do we.
pub fn torus(radius: f32, tube: f32, radial: u32, tubular: u32, arc: f32) -> Mesh {
    let mut m = Mesh::default();
    let (rs, ts) = (radial.max(3), tubular.max(3));
    for j in 0..=rs {
        for i in 0..=ts {
            let u = i as f32 / ts as f32 * arc;
            let v = j as f32 / rs as f32 * std::f32::consts::TAU;
            let p = Vec3::new(
                (radius + tube * v.cos()) * u.cos(),
                (radius + tube * v.cos()) * u.sin(),
                tube * v.sin(),
            );
            let centre = Vec3::new(radius * u.cos(), radius * u.sin(), 0.0);
            m.push(
                p,
                (p - centre).normalize(),
                Vec2::new(i as f32 / ts as f32, j as f32 / rs as f32),
            );
        }
    }
    for j in 1..=rs {
        for i in 1..=ts {
            let a = (ts + 1) * j + i - 1;
            let b = (ts + 1) * (j - 1) + i - 1;
            let c = (ts + 1) * (j - 1) + i;
            let d = (ts + 1) * j + i;
            m.indices.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    m
}

/// A flat ring in the XY plane facing +Z (three.js RingGeometry); rotate it to lie flat.
pub fn ring(inner: f32, outer: f32, segments: u32) -> Mesh {
    let mut m = Mesh::default();
    let seg = segments.max(3);
    for s in 0..=seg {
        let a = s as f32 / seg as f32 * std::f32::consts::TAU;
        let (sa, ca) = a.sin_cos();
        m.push(
            Vec3::new(inner * ca, inner * sa, 0.0),
            Vec3::Z,
            Vec2::new(0.0, s as f32 / seg as f32),
        );
        m.push(
            Vec3::new(outer * ca, outer * sa, 0.0),
            Vec3::Z,
            Vec2::new(1.0, s as f32 / seg as f32),
        );
    }
    for s in 0..seg {
        let a = s * 2;
        m.indices
            .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
    m
}
/// A filled circle in the XY plane facing +Z.
pub fn circle(radius: f32, segments: u32) -> Mesh {
    ring(0.0, radius, segments)
}
/// A plane in the XY plane facing +Z, `w` by `h`, centred.
pub fn plane(w: f32, h: f32) -> Mesh {
    let mut m = Mesh::default();
    let (x, y) = (w / 2.0, h / 2.0);
    m.push(Vec3::new(-x, -y, 0.0), Vec3::Z, Vec2::new(0.0, 0.0));
    m.push(Vec3::new(x, -y, 0.0), Vec3::Z, Vec2::new(1.0, 0.0));
    m.push(Vec3::new(x, y, 0.0), Vec3::Z, Vec2::new(1.0, 1.0));
    m.push(Vec3::new(-x, y, 0.0), Vec3::Z, Vec2::new(0.0, 1.0));
    m.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    m
}

/// Grid lines on the XZ plane, `size` wide with `divisions` cells (three.js GridHelper), as line segments.
pub fn grid_lines(size: f32, divisions: u32) -> Vec<f32> {
    let mut v = Vec::new();
    let half = size / 2.0;
    let step = size / divisions.max(1) as f32;
    for i in 0..=divisions {
        let k = -half + i as f32 * step;
        v.extend_from_slice(&[-half, 0.0, k, half, 0.0, k, k, 0.0, -half, k, 0.0, half]);
    }
    v
}
/// A circle of line segments in the XZ plane at height y (the rims of the plinth).
pub fn circle_lines(radius: f32, y: f32, segments: u32) -> Vec<f32> {
    let mut v = Vec::new();
    for s in 0..segments {
        let a0 = s as f32 / segments as f32 * std::f32::consts::TAU;
        let a1 = (s + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        v.extend_from_slice(&[
            radius * a0.sin(),
            y,
            radius * a0.cos(),
            radius * a1.sin(),
            y,
            radius * a1.cos(),
        ]);
    }
    v
}

/// Catmull-Rom sampling through points (three.js SplineCurve, uniform), `per` samples per span.
pub fn spline_thru(points: &[Vec2], per: usize) -> Vec<Vec2> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..n - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(n - 1)];
        for s in 0..per {
            let t = s as f32 / per as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let v = 0.5
                * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
            out.push(v);
        }
    }
    out.push(points[n - 1]);
    out
}

/// Ear-clipping triangulation of a simple polygon (either winding); returns index triples.
pub fn triangulate(poly: &[Vec2]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            a.x * b.y - b.x * a.y
        })
        .sum();
    let ccw = area > 0.0;
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            let convex = if ccw { cross > 1e-9 } else { cross < -1e-9 };
            if !convex {
                continue;
            }
            let inside = idx
                .iter()
                .any(|&j| j != ia && j != ib && j != ic && point_in_tri(poly[j], a, b, c));
            if inside {
                continue;
            }
            out.push(if ccw { [ia, ib, ic] } else { [ia, ic, ib] });
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        } // degenerate input: stop rather than spin
    }
    if idx.len() == 3 {
        out.push(if ccw {
            [idx[0], idx[1], idx[2]]
        } else {
            [idx[0], idx[2], idx[1]]
        });
    }
    out
}
fn point_in_tri(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let s = |p1: Vec2, p2: Vec2, p3: Vec2| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let d1 = s(p, a, b);
    let d2 = s(p, b, c);
    let d3 = s(p, c, a);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// A fin: the hand-off's `finShape(top, back)`; `u` runs back along the spine (world −z), `y` is
/// height. Built directly in the x = 0 plane, double-sided by the material.
pub fn fin(top: &[(f32, f32)], back: &[(f32, f32)]) -> Mesh {
    let mut outline: Vec<Vec2> = Vec::new();
    let top_pts: Vec<Vec2> = top.iter().map(|&(u, y)| Vec2::new(u, y)).collect();
    outline.extend(spline_thru(&top_pts, 8));
    if !back.is_empty() {
        let mut back_pts = vec![*outline.last().unwrap()];
        back_pts.extend(back.iter().map(|&(u, y)| Vec2::new(u, y)));
        let sampled = spline_thru(&back_pts, 8);
        outline.extend(sampled.into_iter().skip(1));
    }
    // drop near-duplicates
    outline.dedup_by(|a, b| (*a - *b).length_squared() < 1e-8);
    if outline.len() > 2 && (outline[0] - *outline.last().unwrap()).length_squared() < 1e-8 {
        outline.pop();
    }
    let tris = triangulate(&outline);
    let mut m = Mesh::default();
    for p in &outline {
        m.push(Vec3::new(0.0, p.y, -p.x), Vec3::X, Vec2::new(p.x, p.y));
    }
    for t in tris {
        m.indices
            .extend_from_slice(&[t[0] as u32, t[1] as u32, t[2] as u32]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    fn normals_unit(m: &Mesh) -> bool {
        m.normals
            .chunks(3)
            .all(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-3)
    }
    #[test]
    fn sphere_has_unit_normals_and_triangles() {
        let m = sphere(0.1, 16, 12);
        assert!(m.triangle_count() > 100 && normals_unit(&m));
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertex_count()));
    }
    #[test]
    fn capsule_spans_its_length_and_radius() {
        let m = capsule(0.088, 0.26, 8, 20);
        let ys: Vec<f32> = m.positions.chunks(3).map(|p| p[1]).collect();
        let top = ys.iter().cloned().fold(f32::MIN, f32::max);
        let bottom = ys.iter().cloned().fold(f32::MAX, f32::min);
        assert!((top - 0.218).abs() < 1e-3 && (bottom + 0.218).abs() < 1e-3);
        assert!(normals_unit(&m));
    }
    #[test]
    fn cylinder_and_cone_cap_correctly() {
        let c = cylinder(0.018, 0.018, 0.95, 10);
        assert!(c.triangle_count() == 10 * 2 + 10 * 2);
        let k = cone(0.012, 0.03, 8);
        assert!(k.triangle_count() == 8 * 2 + 8);
    }
    #[test]
    fn torus_arc_and_ring() {
        let t = torus(0.05, 0.0035, 6, 28, std::f32::consts::PI);
        assert!(t.triangle_count() == 6 * 28 * 2 && normals_unit(&t));
        let r = ring(0.5, 0.515, 72);
        assert_eq!(r.triangle_count(), 72 * 2);
    }
    #[test]
    fn grid_and_edges_are_segments() {
        assert_eq!(grid_lines(12.0, 48).len(), 49 * 2 * 6);
        assert_eq!(cuboid_edges(1.0, 1.0, 1.0).len(), 12 * 6);
        assert_eq!(cuboid(1.0, 2.0, 3.0).triangle_count(), 12);
    }
    #[test]
    fn fins_triangulate_without_degenerates() {
        let dorsal = fin(
            &[
                (-0.08, 0.1),
                (-0.02, 0.21),
                (0.08, 0.255),
                (0.19, 0.265),
                (0.3, 0.24),
            ],
            &[(0.3, 0.09), (0.1, 0.085)],
        );
        assert!(dorsal.triangle_count() > 10, "{}", dorsal.triangle_count());
        let caudal = fin(
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
        );
        assert!(caudal.triangle_count() > 10);
        assert!(dorsal.positions.chunks(3).all(|p| p[0].abs() < 1e-6));
    }
    #[test]
    fn triangulates_a_square_both_windings() {
        let sq = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        assert_eq!(triangulate(&sq).len(), 2);
        let mut cw = sq.to_vec();
        cw.reverse();
        assert_eq!(triangulate(&cw).len(), 2);
    }
}
