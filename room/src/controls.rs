//! Input geometry shared by flat pointers, pads and XR rays.
use crate::panel::Action;
#[derive(Clone, Debug)]
pub struct Pointer {
    pub start: (f64, f64),
    last: (f64, f64),
    pub action: Option<Action>,
    pub hud: bool,
    threshold: f64,
    moved: bool,
}
impl Pointer {
    pub fn new(x: f64, y: f64, touch: bool, hud: bool, action: Option<Action>) -> Self {
        Self {
            start: (x, y),
            last: (x, y),
            action,
            hud,
            threshold: if touch { 14.0 } else { 10.0 },
            moved: false,
        }
    }
    pub fn motion(&mut self, x: f64, y: f64) -> Option<(f64, f64)> {
        let delta = (x - self.last.0, y - self.last.1);
        self.last = (x, y);
        self.moved |= (x - self.start.0).hypot(y - self.start.1) > self.threshold;
        (self.moved && !self.hud).then_some(delta)
    }
    pub fn click(&self, x: f64, y: f64, action: Option<&Action>) -> bool {
        let distance = (x - self.start.0).hypot(y - self.start.1);
        if self.hud {
            distance <= 24.0 && self.action.as_ref() == action
        } else {
            !self.moved && distance <= self.threshold
        }
    }
}
/// Nearest visible control in the requested direction, with orthogonal distance weighted.
pub fn navigate(points: &[(f64, f64)], current: usize, dx: f64, dy: f64) -> usize {
    let Some(&(x, y)) = points.get(current) else {
        return 0;
    };
    points
        .iter()
        .enumerate()
        .filter_map(|(i, &(px, py))| {
            let (a, b) = (px - x, py - y);
            let forward = a * dx + b * dy;
            (forward > 1.0).then_some((i, forward + 3.0 * (a * dy - b * dx).abs()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|p| p.0)
        .unwrap_or(current)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jitter_keeps_same_control_and_drag_never_orbits_hud() {
        let a = Action::Control(0);
        let mut p = Pointer::new(10., 10., false, true, Some(a.clone()));
        assert_eq!(p.motion(16., 15.), None);
        assert!(p.click(16., 15., Some(&a)));
        assert!(!p.click(16., 15., Some(&Action::Control(1))));
        assert_eq!(p.motion(40., 10.), None);
        assert!(!p.click(40., 10., Some(&a)));
        let mut p = Pointer::new(0., 0., false, false, None);
        assert!(p.motion(25., 0.).is_some());
        assert!(!p.click(25., 0., None));
    }
    #[test]
    fn grid_focus_reaches_every_control() {
        let p = [(0., 0.), (50., 0.), (0., 50.), (50., 50.)];
        assert_eq!(navigate(&p, 0, 1., 0.), 1);
        assert_eq!(navigate(&p, 1, 0., 1.), 3);
        assert_eq!(navigate(&p, 3, -1., 0.), 2);
        assert_eq!(navigate(&p, 2, 0., -1.), 0);
    }
}
