//! The launch strip's kinematics (roadmap/N8i.md): a countdown, then 0–100 km/h in 2.5 s at a
//! constant average acceleration, a second of cruise, and a gentle stop. Closed form, so the
//! speedometer reads the profile at the frame's own time and never drifts.

/// The figure he asked to feel.
pub const TARGET_KMH: f32 = 100.0;
pub const TARGET_S: f32 = 2.5;
pub const COUNTDOWN_S: f32 = 3.0;
pub const CRUISE_S: f32 = 1.0;
/// Standard gravity, for the "g" on the dashboard.
pub const G: f32 = 9.806_65;
/// Gentle braking, 0.3 g.
pub const BRAKE: f32 = 0.3 * G;

/// The average acceleration of 0–100 km/h in 2.5 s: 11.11 m/s², about 1.13 g.
pub fn accel() -> f32 {
    TARGET_KMH / 3.6 / TARGET_S
}
pub fn top() -> f32 {
    TARGET_KMH / 3.6
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Countdown,
    Launch,
    Cruise,
    Brake,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub phase: Phase,
    /// Seconds since "go"; negative during the countdown.
    pub t: f32,
    /// Metres per second.
    pub speed: f32,
    /// Metres from the start line.
    pub distance: f32,
    /// Metres per second squared, signed.
    pub accel: f32,
    /// 3, 2, 1 during the countdown, else 0.
    pub count: u32,
}
impl Sample {
    pub fn kmh(&self) -> f32 {
        self.speed * 3.6
    }
}

/// Where the profile is `since_press` seconds after he pressed launch.
pub fn sample(since_press: f32) -> Sample {
    let a = accel();
    let v1 = top();
    let t = since_press - COUNTDOWN_S;
    let d1 = 0.5 * a * TARGET_S * TARGET_S;
    let d2 = d1 + v1 * CRUISE_S;
    let stop_after = v1 / BRAKE;
    let base = Sample {
        phase: Phase::Countdown,
        t,
        speed: 0.0,
        distance: 0.0,
        accel: 0.0,
        count: 0,
    };
    if t < 0.0 {
        return Sample {
            count: (-t).ceil() as u32,
            ..base
        };
    }
    if t <= TARGET_S {
        return Sample {
            phase: Phase::Launch,
            speed: a * t,
            distance: 0.5 * a * t * t,
            accel: a,
            ..base
        };
    }
    if t <= TARGET_S + CRUISE_S {
        return Sample {
            phase: Phase::Cruise,
            speed: v1,
            distance: d1 + v1 * (t - TARGET_S),
            ..base
        };
    }
    let tb = (t - TARGET_S - CRUISE_S).min(stop_after);
    let speed = (v1 - BRAKE * tb).max(0.0);
    Sample {
        phase: if tb >= stop_after {
            Phase::Stopped
        } else {
            Phase::Brake
        },
        speed,
        distance: d2 + v1 * tb - 0.5 * BRAKE * tb * tb,
        accel: if speed > 0.0 { -BRAKE } else { 0.0 },
        ..base
    }
}

/// The whole run's length, start line to rest: the road has to be longer than this.
pub fn total_distance() -> f32 {
    sample(COUNTDOWN_S + TARGET_S + CRUISE_S + top() / BRAKE + 1.0).distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundred_at_two_and_a_half_seconds() {
        let s = sample(COUNTDOWN_S + 2.5);
        assert!((s.kmh() - 100.0).abs() < 0.1, "{}", s.kmh());
        assert!((s.distance - 34.72).abs() < 0.1, "{}", s.distance);
        assert!((accel() / G - 1.133).abs() < 0.01);
        // one 72 Hz frame earlier is visibly short of 100
        assert!(sample(COUNTDOWN_S + 2.5 - 1.0 / 72.0).kmh() < 99.7);
    }

    #[test]
    fn countdown_then_launch() {
        assert_eq!(sample(0.0).count, 3);
        assert_eq!(sample(1.5).count, 2);
        assert_eq!(sample(2.9).count, 1);
        assert_eq!(sample(0.5).speed, 0.0);
        assert_eq!(sample(3.01).phase, Phase::Launch);
    }

    #[test]
    fn stops_and_never_goes_backwards() {
        let mut last = sample(0.0);
        let mut t = 0.0;
        while t < 20.0 {
            let s = sample(t);
            assert!(s.speed >= 0.0);
            assert!(s.distance >= last.distance - 1e-4);
            last = s;
            t += 1.0 / 90.0;
        }
        assert_eq!(last.phase, Phase::Stopped);
        assert_eq!(last.speed, 0.0);
        let total = total_distance();
        assert!(total > 150.0 && total < 250.0, "{total}");
    }
}
