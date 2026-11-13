//! The garage (roadmap/N8i.md): a car in my room three ways. A seated launch strip in VR (and on
//! a flat screen), a life-size car on the visitor's floor, and a 1:10 car they drive through
//! their home. The car is a generic SUV of my own and its sound is synthesised here; VR can't
//! give the push of 1.13 g, so the launch is honest kinematics with comfort first.

pub mod car;
pub mod drive;
pub mod launch;
pub mod planes;

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Launch,
    LifeSize,
    Miniature,
}
impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "launch" => Some(Mode::Launch),
            "life" => Some(Mode::LifeSize),
            "mini" => Some(Mode::Miniature),
            _ => None,
        }
    }
    /// The launch strip is a place of its own; the other two live in the visitor's room.
    pub fn ar(self) -> bool {
        self != Mode::Launch
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
        assert!(!Mode::Launch.ar() && Mode::Miniature.ar());
        assert_eq!(Mode::Miniature.scale(), 0.1);
    }
}
