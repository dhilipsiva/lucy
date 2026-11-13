//! The garage (roadmap/N8i.md): a car in my room three ways. A seated launch strip in VR (and on
//! a flat screen), a life-size car on the visitor's floor, and a 1:10 car they drive through
//! their home; and, since roadmap/N8j.md, capturing a home in mixed reality and looking at it
//! again. The car is a generic SUV of my own and its sound is synthesised here; VR can't
//! give the push of 1.13 g, so the launch is honest kinematics with comfort first.

pub use lucy_room_memory::blur;

pub mod car;
pub mod drive;
pub mod launch;
pub mod planes;
pub mod project;

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
mod homecap;
#[cfg(target_arch = "wasm32")]
pub use browser::*;
#[cfg(target_arch = "wasm32")]
pub use homecap::{store as store_capture, Capturing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Launch,
    LifeSize,
    Miniature,
    /// Walking a home in mixed reality while it is recorded (roadmap/N8j.md).
    Capture,
    /// Looking at a captured home.
    Home,
}
impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "launch" => Some(Mode::Launch),
            "life" => Some(Mode::LifeSize),
            "mini" => Some(Mode::Miniature),
            "capture" => Some(Mode::Capture),
            "home" | "home-vr" => Some(Mode::Home),
            _ => None,
        }
    }
    /// The launch strip and a captured home are places of their own; the rest happen in the
    /// visitor's room.
    pub fn ar(self) -> bool {
        matches!(self, Mode::LifeSize | Mode::Miniature | Mode::Capture)
    }
    pub fn scale(self) -> f32 {
        if self == Mode::Miniature {
            0.1
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_from_the_panel() {
        assert_eq!(Mode::parse("launch"), Some(Mode::Launch));
        assert_eq!(Mode::parse("life"), Some(Mode::LifeSize));
        assert_eq!(Mode::parse("mini"), Some(Mode::Miniature));
        assert_eq!(Mode::parse("drag"), None);
        assert_eq!(Mode::parse("capture"), Some(Mode::Capture));
        assert_eq!(Mode::parse("home-vr"), Some(Mode::Home));
        assert!(Mode::Capture.ar() && !Mode::Home.ar());
        assert!(!Mode::Launch.ar() && Mode::Miniature.ar());
        assert_eq!(Mode::Miniature.scale(), 0.1);
    }
}
