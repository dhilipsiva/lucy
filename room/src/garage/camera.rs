//! Which camera a capture should use, and whether its pictures are a view of anything
//! (roadmap/N8j.v3.md). On his Quest 3 the camera a page gets by default is labelled
//! "camera 0, facing front" and gave the same pastel picture 57 times: a stand-in, not his
//! room. A capture asks for a camera that faces the world when one is offered, and says so
//! when the pictures it keeps all look alike.

/// How much a label sounds like a camera that looks at the room: higher is better.
fn outward(label: &str) -> i32 {
    let l = label.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| l.contains(w));
    if has(&[
        "back",
        "rear",
        "environment",
        "world",
        "passthrough",
        "headset",
    ]) {
        2
    } else if has(&["front", "user", "selfie", "avatar", "virtual"]) {
        0
    } else {
        1
    }
}

/// The listed camera to switch to, if one sounds more outward-facing than the one in use.
pub fn better(labels: &[String], current: &str) -> Option<usize> {
    let (best, score) = labels
        .iter()
        .enumerate()
        .map(|(i, l)| (i, outward(l)))
        .max_by_key(|&(i, s)| (s, std::cmp::Reverse(i)))?;
    (score > outward(current) && labels[best] != current).then_some(best)
}

/// Whether a camera's label says it faces the wearer, so a world-facing one is worth asking for.
pub fn faces_wearer(label: &str) -> bool {
    outward(label) == 0
}

/// The sizes of the JPEGs kept so far. Pictures of a room being walked through differ in size
/// by tens of percent; a stand-in picture encodes to almost the same bytes every time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sizes {
    pub count: u32,
    pub min: usize,
    pub max: usize,
}

impl Sizes {
    pub fn add(&mut self, bytes: usize) {
        self.min = if self.count == 0 {
            bytes
        } else {
            self.min.min(bytes)
        };
        self.max = self.max.max(bytes);
        self.count += 1;
    }
    /// Eight or more pictures within one percent of each other in size.
    pub fn all_alike(&self) -> bool {
        self.count >= 8 && (self.max - self.min) * 100 <= self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn his_quest_offers_nothing_better_than_its_one_camera() {
        // roadmap/N8h-evidence.json: the only video input his headset listed
        let l = labels(&["camera 0, facing front"]);
        assert_eq!(better(&l, "camera 0, facing front"), None);
        assert!(faces_wearer("camera 0, facing front"));
    }

    #[test]
    fn a_world_facing_camera_is_preferred_when_listed() {
        let l = labels(&["camera 0, facing front", "camera 1, facing back"]);
        assert_eq!(better(&l, "camera 0, facing front"), Some(1));
        assert_eq!(better(&l, "camera 1, facing back"), None);
        let l = labels(&["Avatar Camera", "Headset Passthrough Camera"]);
        assert_eq!(better(&l, "Avatar Camera"), Some(1));
    }

    #[test]
    fn an_unknown_label_is_not_swapped_for_another_unknown() {
        let l = labels(&["HD Webcam", "USB Camera"]);
        assert_eq!(better(&l, "HD Webcam"), None);
        assert!(!faces_wearer("HD Webcam"));
        // but it does beat one that faces the wearer
        let l = labels(&["FaceTime front camera", "USB Camera"]);
        assert_eq!(better(&l, "FaceTime front camera"), Some(1));
    }

    #[test]
    fn the_same_picture_again_and_again_is_noticed() {
        // his capture of 2026-11-14: 57 pictures between 21151 and 21183 bytes
        let mut s = Sizes::default();
        for b in [21151, 21171, 21183, 21160, 21155, 21180, 21166, 21170] {
            s.add(b);
        }
        assert!(s.all_alike());
    }

    #[test]
    fn a_walk_through_a_room_is_not_mistaken_for_one_picture() {
        let mut s = Sizes::default();
        for b in [41000, 52000, 38000, 61000, 47000, 44000, 58000, 36000] {
            s.add(b);
        }
        assert!(!s.all_alike());
        // and too few pictures say nothing yet
        let mut few = Sizes::default();
        for b in [21151, 21171, 21183] {
            few.add(b);
        }
        assert!(!few.all_alike());
    }
}
