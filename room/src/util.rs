//! Small shared helpers: easing, clamping, angles, and a tiny deterministic random source (no
//! crate; the room needs only a little jitter for blinks, hops and glances).

pub fn ease(x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else if x >= 1.0 {
        1.0
    } else {
        1.0 - (1.0 - x).powi(3)
    }
}
pub fn clamp(v: f32, a: f32, b: f32) -> f32 {
    v.max(a).min(b)
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
/// The shortest signed turn from `a` to `b`, in radians.
pub fn angle_delta(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// xorshift64*: enough randomness for animation, the same on every platform.
#[derive(Clone, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1) ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ease_is_monotone_and_clamped() {
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
        assert!(ease(0.3) < ease(0.6));
    }
    #[test]
    fn angle_delta_takes_the_short_way() {
        assert!((angle_delta(0.1, std::f32::consts::TAU - 0.1) + 0.2).abs() < 1e-5);
    }
    #[test]
    fn rng_is_in_range_and_deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            let x = a.f32();
            assert!((0.0..1.0).contains(&x));
            assert_eq!(x, b.f32());
        }
    }
}
