//! The miniature's driving (roadmap/N8i.md): a kinematic bicycle model on the floor plane, and
//! a capsule-shaped body that stops at wall and furniture segments. Floor coordinates are
//! (x, z) in metres; yaw 0 faces −Z, like the room's camera.

use glam::Vec2;

/// One obstacle edge on the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: Vec2,
    pub b: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// 0..1
    pub throttle: f32,
    /// 0..1: brakes while moving forward, reverses from rest.
    pub brake: f32,
    /// −1 (left) .. 1 (right)
    pub steer: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Car {
    pub pos: Vec2,
    pub yaw: f32,
    /// Metres per second along the heading; negative in reverse.
    pub speed: f32,
    /// 1.0 for life-size, 0.1 for the miniature.
    pub scale: f32,
}

/// The placeholder SUV's footprint at 1:1 (car.rs builds the same size).
pub const LENGTH: f32 = 4.9;
pub const WIDTH: f32 = 2.0;
pub const WHEELBASE: f32 = 2.9;
const MAX_STEER: f32 = 0.52; // 30°
/// At 1:10: a brisk toy, not a scale 250 km/h.
const TOP: f32 = 3.0;
const REVERSE_TOP: f32 = 1.0;
const ACCEL: f32 = 2.5;
const BRAKE: f32 = 4.0;
const ROLL: f32 = 0.6;
const BOUNCE: f32 = 0.3;

impl Car {
    pub fn new(pos: Vec2, yaw: f32, scale: f32) -> Car {
        Car {
            pos,
            yaw,
            speed: 0.0,
            scale,
        }
    }
    pub fn forward(&self) -> Vec2 {
        Vec2::new(-self.yaw.sin(), -self.yaw.cos())
    }
    fn radius(&self) -> f32 {
        WIDTH * self.scale / 2.0
    }
    /// The body as a capsule: the segment between these two centres, swept by `radius`.
    fn spine(&self) -> (Vec2, Vec2) {
        let half = (LENGTH * self.scale / 2.0 - self.radius()).max(0.0);
        let f = self.forward() * half;
        (self.pos - f, self.pos + f)
    }

    /// Advance by `dt` seconds; true when the car touched an obstacle.
    pub fn step(&mut self, c: Controls, dt: f32, walls: &[Segment]) -> bool {
        let top = TOP * self.scale / 0.1;
        let reverse_top = REVERSE_TOP * self.scale / 0.1;
        let accel = ACCEL * self.scale / 0.1;
        let brake = BRAKE * self.scale / 0.1;
        let roll = ROLL * self.scale / 0.1;
        let throttle = c.throttle.clamp(0.0, 1.0);
        let pedal = c.brake.clamp(0.0, 1.0);
        let mut v = self.speed;
        if throttle > 0.0 {
            v += throttle * if v < 0.0 { brake } else { accel } * dt;
        }
        if pedal > 0.0 {
            v -= pedal * if v > 0.0 { brake } else { accel } * dt;
        }
        if throttle == 0.0 && pedal == 0.0 {
            v -= v.signum() * (roll * dt).min(v.abs());
        }
        self.speed = v.clamp(-reverse_top, top);
        let wheelbase = WHEELBASE * self.scale;
        let steer = c.steer.clamp(-1.0, 1.0) * MAX_STEER;
        // Substeps: never move more than a quarter of the body's width at once, so a wall can't be
        // jumped over in one frame.
        let travel = (self.speed * dt).abs();
        let n = ((travel / (self.radius() * 0.5)).ceil() as usize).clamp(1, 64);
        let h = dt / n as f32;
        let mut hit = false;
        for _ in 0..n {
            self.yaw -= self.speed / wheelbase * steer.tan() * h;
            self.pos += self.forward() * self.speed * h;
            hit |= self.resolve(walls);
        }
        hit
    }

    /// Push the body out of any segment it overlaps; bounce off if it was driving into it.
    fn resolve(&mut self, walls: &[Segment]) -> bool {
        let r = self.radius();
        let mut hit = false;
        for w in walls {
            let (p, q) = self.spine();
            let (on_car, on_wall) = closest(p, q, w.a, w.b);
            let gap = on_car - on_wall;
            let d = gap.length();
            if d >= r {
                continue;
            }
            let normal = if d > 1e-6 {
                gap / d
            } else {
                // centred on the wall: push back the way it came
                -self.forward() * self.speed.signum()
            };
            self.pos += normal * (r - d + 1e-4);
            let moving = self.forward() * self.speed;
            if moving.dot(normal) < 0.0 {
                self.speed = -self.speed * BOUNCE;
            }
            hit = true;
        }
        hit
    }
}

fn point_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}
/// Closest points between segments pq and ab (the first on pq, the second on ab).
fn closest(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> (Vec2, Vec2) {
    let (r, s) = (q - p, b - a);
    let denom = cross(r, s);
    if denom.abs() > 1e-9 {
        let t = cross(a - p, s) / denom;
        let u = cross(a - p, r) / denom;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            let x = p + r * t;
            return (x, x);
        }
    }
    let candidates = [
        (p, point_segment(p, a, b)),
        (q, point_segment(q, a, b)),
        (point_segment(a, p, q), a),
        (point_segment(b, p, q), b),
    ];
    candidates
        .into_iter()
        .min_by(|x, y| (x.0 - x.1).length().total_cmp(&(y.0 - y.1).length()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_ahead(z: f32) -> Segment {
        Segment {
            a: Vec2::new(-5.0, z),
            b: Vec2::new(5.0, z),
        }
    }

    #[test]
    fn full_throttle_never_passes_a_wall() {
        let walls = [wall_ahead(-2.0)];
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        let full = Controls {
            throttle: 1.0,
            ..Default::default()
        };
        let mut touched = false;
        for _ in 0..(10 * 72) {
            touched |= car.step(full, 1.0 / 72.0, &walls);
            // the nose (front of the body) stays on the near side
            let nose = car.pos.y - LENGTH * car.scale / 2.0;
            assert!(nose > -2.0 - 1e-3, "nose {nose}");
        }
        assert!(touched);
        assert!(car.pos.y > -2.0 + LENGTH * 0.1 / 2.0 - 0.02);
    }

    #[test]
    fn a_huge_frame_cannot_tunnel() {
        let walls = [wall_ahead(-1.0)];
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        car.speed = 3.0;
        car.step(
            Controls {
                throttle: 1.0,
                ..Default::default()
            },
            0.1,
            &walls,
        );
        car.step(Controls::default(), 1.0, &walls);
        assert!(car.pos.y > -1.0);
    }

    #[test]
    fn pushed_out_along_the_normal() {
        let walls = [Segment {
            a: Vec2::new(0.05, -1.0),
            b: Vec2::new(0.05, 1.0),
        }];
        // overlapping a wall at its right side, parked
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        assert!(car.step(Controls::default(), 1.0 / 72.0, &walls));
        assert!(
            (car.pos.x - (0.05 - WIDTH * 0.1 / 2.0)).abs() < 0.01,
            "{}",
            car.pos.x
        );
        assert!(car.pos.y.abs() < 1e-3);
    }

    #[test]
    fn brake_from_rest_reverses_and_steering_turns() {
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        for _ in 0..72 {
            car.step(
                Controls {
                    brake: 1.0,
                    ..Default::default()
                },
                1.0 / 72.0,
                &[],
            );
        }
        assert!(car.speed < 0.0 && car.speed >= -1.0);
        assert!(car.pos.y > 0.0, "reversed toward +z");
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        for _ in 0..72 {
            car.step(
                Controls {
                    throttle: 1.0,
                    steer: 1.0,
                    ..Default::default()
                },
                1.0 / 72.0,
                &[],
            );
        }
        assert!(
            car.yaw < -0.2,
            "right steer turns clockwise from above: {}",
            car.yaw
        );
        assert!(car.pos.x > 0.0);
    }

    #[test]
    fn coasting_stops() {
        let mut car = Car::new(Vec2::ZERO, 0.0, 0.1);
        car.speed = 2.0;
        for _ in 0..(10 * 72) {
            car.step(Controls::default(), 1.0 / 72.0, &[]);
        }
        assert_eq!(car.speed, 0.0);
    }
}
