//! Which camera frames of a home capture to keep (roadmap/N8j.md): one each time the wearer
//! has moved or turned enough since the last, and only while the head turns slowly, so the
//! pictures are spread over the home and sharp.

use glam::Vec3;

pub const STEP_M: f32 = 0.4;
pub const TURN_RAD: f32 = 20.0 * std::f32::consts::PI / 180.0;
/// Faster than this and the picture smears.
pub const MAX_TURN_RATE: f32 = 60.0 * std::f32::consts::PI / 180.0;

#[derive(Default)]
pub struct Selector {
    last: Option<(Vec3, Vec3)>,
    kept: usize,
}
impl Selector {
    /// `forward` is the head's unit view direction; `turn_rate` in radians a second.
    pub fn wants(&self, pos: Vec3, forward: Vec3, turn_rate: f32, cap: usize) -> bool {
        if self.kept >= cap || turn_rate > MAX_TURN_RATE {
            return false;
        }
        match self.last {
            None => true,
            Some((p, f)) => {
                pos.distance(p) >= STEP_M || f.dot(forward).clamp(-1.0, 1.0).acos() >= TURN_RAD
            }
        }
    }
    /// A frame was taken from here (kept or dropped: either way, move on).
    pub fn took(&mut self, pos: Vec3, forward: Vec3) {
        self.last = Some((pos, forward));
        self.kept += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaced_by_distance_or_turn_and_never_while_spinning() {
        let mut s = Selector::default();
        let f = Vec3::NEG_Z;
        assert!(s.wants(Vec3::ZERO, f, 0.0, 400));
        s.took(Vec3::ZERO, f);
        assert!(!s.wants(Vec3::new(0.2, 0.0, 0.0), f, 0.0, 400));
        assert!(s.wants(Vec3::new(0.41, 0.0, 0.0), f, 0.0, 400));
        let turned = Vec3::new(
            -(25f32.to_radians()).sin(),
            0.0,
            -(25f32.to_radians()).cos(),
        );
        assert!(s.wants(Vec3::ZERO, turned, 0.0, 400));
        assert!(!s.wants(Vec3::ZERO, turned, 2.0, 400), "spinning");
        assert!(!s.wants(Vec3::new(5.0, 0.0, 0.0), f, 0.0, 1), "cap reached");
    }
}
