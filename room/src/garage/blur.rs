//! Faces out of a picture before it is kept (roadmap/N8j.md). Each face box is widened and
//! pixelated in place. The rule that matters is `process`: without a detection that ran and
//! answered, there is no picture.

/// A face box, corners in 0..1 of the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// What the face finder said about one picture.
#[derive(Clone, Debug, PartialEq)]
pub enum Detection {
    Found(Vec<FaceBox>),
    Failed(String),
    TimedOut,
    /// No detector at all (it did not load, or this site does not carry it).
    Missing,
}

/// How much wider than the detector's box to blur, as a share of the box each side.
pub const MARGIN: f32 = 0.35;
/// Blocks across the widened box: few enough that no face survives.
pub const BLOCKS: usize = 6;

/// Blur the faces and say how many, or refuse the picture. Fail closed: only `Found` keeps it.
pub fn process(
    rgba: &mut [u8],
    width: usize,
    height: usize,
    detection: &Detection,
) -> Result<u32, String> {
    match detection {
        Detection::Found(boxes) => {
            if width == 0 || height == 0 || rgba.len() != width * height * 4 {
                return Err("the picture's size does not match its bytes".into());
            }
            for b in boxes {
                pixelate(rgba, width, height, b);
            }
            Ok(boxes.len() as u32)
        }
        Detection::Failed(e) => Err(format!("the face finder failed: {e}")),
        Detection::TimedOut => Err("the face finder took too long".into()),
        Detection::Missing => Err("no face finder is loaded".into()),
    }
}

/// The widened box in pixels: x0, y0, x1, y1 (exclusive).
pub fn region(b: &FaceBox, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let (bw, bh) = ((b.x2 - b.x1).abs(), (b.y2 - b.y1).abs());
    let px = |v: f32, n: usize| (v.clamp(0.0, 1.0) * n as f32).round() as usize;
    let x0 = px(b.x1.min(b.x2) - bw * MARGIN, width);
    let x1 = px(b.x1.max(b.x2) + bw * MARGIN, width)
        .max(x0 + 1)
        .min(width);
    let y0 = px(b.y1.min(b.y2) - bh * MARGIN, height);
    let y1 = px(b.y1.max(b.y2) + bh * MARGIN, height)
        .max(y0 + 1)
        .min(height);
    (x0.min(width - 1), y0.min(height - 1), x1, y1)
}

fn pixelate(rgba: &mut [u8], width: usize, height: usize, b: &FaceBox) {
    let (x0, y0, x1, y1) = region(b, width, height);
    let block = ((x1 - x0).max(y1 - y0)).div_ceil(BLOCKS).max(4);
    let mut by = y0;
    while by < y1 {
        let mut bx = x0;
        while bx < x1 {
            let (ex, ey) = ((bx + block).min(x1), (by + block).min(y1));
            let mut sum = [0u32; 3];
            let mut n = 0u32;
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * width + x) * 4;
                    for c in 0..3 {
                        sum[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * width + x) * 4;
                    for c in 0..3 {
                        rgba[i + c] = (sum[c] / n.max(1)) as u8;
                    }
                }
            }
            bx += block;
        }
        by += block;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy(w: usize, h: usize) -> Vec<u8> {
        (0..w * h * 4)
            .map(|i| ((i * 2654435761usize) >> 7) as u8)
            .collect()
    }

    #[test]
    fn a_face_box_is_flattened_and_the_rest_untouched() {
        let (w, h) = (64, 48);
        let src = noisy(w, h);
        let mut px = src.clone();
        let b = FaceBox {
            x1: 0.4,
            y1: 0.4,
            x2: 0.6,
            y2: 0.6,
        };
        assert_eq!(process(&mut px, w, h, &Detection::Found(vec![b])), Ok(1));
        let (x0, y0, x1, y1) = region(&b, w, h);
        // wider than the detector's own box
        assert!(x0 < (0.4 * w as f32) as usize && x1 > (0.6 * w as f32) as usize);
        let mut distinct = std::collections::BTreeSet::new();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                if x >= x0 && x < x1 && y >= y0 && y < y1 {
                    distinct.insert([px[i], px[i + 1], px[i + 2]]);
                } else {
                    assert_eq!(px[i..i + 4], src[i..i + 4], "outside the box at {x},{y}");
                }
            }
        }
        // at most BLOCKS x BLOCKS flat colours where there were hundreds
        assert!(
            distinct.len() <= (BLOCKS + 1) * (BLOCKS + 1),
            "{}",
            distinct.len()
        );
    }

    #[test]
    fn no_answer_means_no_picture() {
        let (w, h) = (8, 8);
        let src = noisy(w, h);
        for d in [
            Detection::Failed("x".into()),
            Detection::TimedOut,
            Detection::Missing,
        ] {
            let mut px = src.clone();
            assert!(process(&mut px, w, h, &d).is_err());
            assert_eq!(px, src);
        }
        let mut px = src.clone();
        assert_eq!(process(&mut px, w, h, &Detection::Found(vec![])), Ok(0));
        assert_eq!(px, src);
        assert!(process(&mut px, 9, 8, &Detection::Found(vec![])).is_err());
    }

    #[test]
    fn boxes_at_the_edge_stay_inside() {
        let b = FaceBox {
            x1: 0.9,
            y1: -0.2,
            x2: 1.3,
            y2: 0.1,
        };
        let (x0, y0, x1, y1) = region(&b, 640, 480);
        assert!(x0 < x1 && x1 <= 640 && y0 < y1 && y1 <= 480);
        let mut px = noisy(640, 480);
        process(&mut px, 640, 480, &Detection::Found(vec![b])).unwrap();
    }
}
