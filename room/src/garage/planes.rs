//! Detected planes to obstacles on the floor (roadmap/N8i.md). A wall becomes the segment it
//! stands on; a table or couch top becomes the outline of its footprint. These live for one
//! session in memory and are never kept: the shape of his home stays his.

use super::drive::Segment;
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// Horizontal planes lower than this above the floor are the floor itself, a rug or a step.
pub const LOW: f32 = 0.15;

/// `pose` takes the plane's own space to floor space; `polygon` is in plane space (its y is 0).
pub fn obstacles(
    pose: Mat4,
    polygon: &[Vec3],
    orientation: Orientation,
    floor_y: f32,
) -> Vec<Segment> {
    let world: Vec<Vec3> = polygon.iter().map(|p| pose.transform_point3(*p)).collect();
    if world.len() < 2 {
        return vec![];
    }
    let flat: Vec<Vec2> = world.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    match orientation {
        Orientation::Vertical => {
            // the wall's footprint: the two extreme points along its widest direction
            let mut best = (0.0, flat[0], flat[0]);
            for (i, a) in flat.iter().enumerate() {
                for b in &flat[i + 1..] {
                    let d = (*a - *b).length_squared();
                    if d > best.0 {
                        best = (d, *a, *b);
                    }
                }
            }
            if best.0 < 1e-6 {
                return vec![];
            }
            vec![Segment {
                a: best.1,
                b: best.2,
            }]
        }
        Orientation::Horizontal => {
            let height = world.iter().map(|p| p.y).sum::<f32>() / world.len() as f32 - floor_y;
            if height < LOW {
                return vec![];
            }
            (0..flat.len())
                .map(|i| Segment {
                    a: flat[i],
                    b: flat[(i + 1) % flat.len()],
                })
                .filter(|s| (s.a - s.b).length_squared() > 1e-8)
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn square(half: f32) -> Vec<Vec3> {
        vec![
            Vec3::new(-half, 0.0, -half),
            Vec3::new(half, 0.0, -half),
            Vec3::new(half, 0.0, half),
            Vec3::new(-half, 0.0, half),
        ]
    }

    #[test]
    fn a_wall_becomes_the_segment_it_stands_on() {
        // a 2 m wide wall 3 m ahead, facing the viewer: plane normal (+y in plane space) → +z
        let pose = Mat4::from_rotation_translation(
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            Vec3::new(0.0, 1.0, -3.0),
        );
        let segs = obstacles(pose, &square(1.0), Orientation::Vertical, 0.0);
        assert_eq!(segs.len(), 1);
        let s = segs[0];
        assert!((s.a.y + 3.0).abs() < 1e-4 && (s.b.y + 3.0).abs() < 1e-4);
        assert!(((s.a.x - s.b.x).abs() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_table_top_is_its_footprint_and_the_floor_is_nothing() {
        let table = Mat4::from_translation(Vec3::new(1.0, 0.75, 0.0));
        let segs = obstacles(table, &square(0.5), Orientation::Horizontal, 0.0);
        assert_eq!(segs.len(), 4);
        for s in &segs {
            for p in [s.a, s.b] {
                assert!((p.x - 1.0).abs() <= 0.5 + 1e-4 && p.y.abs() <= 0.5 + 1e-4);
            }
        }
        let rug = Mat4::from_translation(Vec3::new(0.0, 0.05, 0.0));
        assert!(obstacles(rug, &square(2.0), Orientation::Horizontal, 0.0).is_empty());
    }
}
