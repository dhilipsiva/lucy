//! The placeholder car (roadmap/N8i.md): a generic boxy SUV of my own, 4.9 × 2.0 × 1.65 m,
//! from my geometry builders. It is nobody's design on purpose; a model he is allowed to use
//! can replace it later. Origin at the middle of its footprint on the ground, nose toward −Z.

use super::drive::{LENGTH, WHEELBASE, WIDTH};
use crate::geom::{cuboid, cylinder, Mesh};
use glam::Vec3;

pub const HEIGHT: f32 = 1.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Body,
    Cabin,
    Wheel,
    Headlight,
    Taillight,
}

/// Every part with its role; the caller picks colours.
pub fn parts(scale: f32) -> Vec<(Part, Mesh)> {
    let wheel_r = 0.38;
    let wheel_w = 0.28;
    let clearance = 0.32;
    let body_h = 0.78;
    let body_w = WIDTH - 0.06;
    let cabin_h = HEIGHT - clearance - body_h;
    let mut out = vec![
        (
            Part::Body,
            cuboid(body_w, body_h, LENGTH).translate(Vec3::new(0.0, clearance + body_h / 2.0, 0.0)),
        ),
        (
            Part::Cabin,
            cuboid(body_w - 0.22, cabin_h, LENGTH * 0.55).translate(Vec3::new(
                0.0,
                clearance + body_h + cabin_h / 2.0,
                0.12,
            )),
        ),
        (
            Part::Headlight,
            cuboid(body_w - 0.3, 0.07, 0.02).translate(Vec3::new(
                0.0,
                clearance + body_h - 0.16,
                -LENGTH / 2.0 + 0.008,
            )),
        ),
        (
            Part::Taillight,
            cuboid(body_w - 0.3, 0.07, 0.02).translate(Vec3::new(
                0.0,
                clearance + body_h - 0.12,
                LENGTH / 2.0 - 0.008,
            )),
        ),
    ];
    for x in [-1.0f32, 1.0] {
        for z in [-1.0f32, 1.0] {
            out.push((
                Part::Wheel,
                axle_x(cylinder(wheel_r, wheel_r, wheel_w, 20)).translate(Vec3::new(
                    x * (WIDTH - wheel_w) / 2.0,
                    wheel_r,
                    z * WHEELBASE / 2.0,
                )),
            ));
        }
    }
    for (_, m) in &mut out {
        for v in m.positions.iter_mut() {
            *v *= scale;
        }
    }
    out
}

/// Turn a Y-axis cylinder to lie along X (a wheel on its axle).
fn axle_x(mut m: Mesh) -> Mesh {
    for v in [&mut m.positions, &mut m.normals] {
        for p in v.chunks_mut(3) {
            let (x, y) = (p[0], p[1]);
            p[0] = y;
            p[1] = -x;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(scale: f32) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (_, m) in parts(scale) {
            for p in m.positions.chunks(3) {
                let v = Vec3::new(p[0], p[1], p[2]);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (lo, hi)
    }

    #[test]
    fn sizes_match_at_full_and_tenth_scale() {
        for scale in [1.0f32, 0.1] {
            let (lo, hi) = bounds(scale);
            let size = hi - lo;
            assert!((size.x - WIDTH * scale).abs() < 0.01 * scale, "{size}");
            assert!((size.y - HEIGHT * scale).abs() < 0.01 * scale, "{size}");
            assert!((size.z - LENGTH * scale).abs() < 0.02 * scale, "{size}");
            assert!(lo.y.abs() < 1e-4, "sits on the ground");
        }
    }
}
