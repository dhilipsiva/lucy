//! Blurring a raw capture on the desktop (roadmap/N8j.v2.md). Every picture is decoded, read
//! by both models, its faces pixelated, and encoded again; the file is then marked processed.
//! One picture that cannot be read or checked fails the whole file: nothing half-blurred is
//! written.

use crate::{detect, load, load_large, merge, Face, LARGE_NAME, LARGE_SHA256, NAME};
use lucy_room_memory::blur::{self, Detection, FaceBox};
use lucy_room_memory::home::{base64_decode, base64_encode, Detector, HomeCapture};

pub struct Blurred {
    pub json: String,
    pub pictures: usize,
    pub faces: u32,
    pub was_raw: bool,
}

pub fn blur_capture(text: &str) -> Result<Blurred, String> {
    let mut capture = HomeCapture::parse(text)?;
    let (small, large) = (load()?, load_large()?);
    let was_raw = capture.raw;
    let mut total = 0;
    for photo in &mut capture.photos {
        if photo.jpeg_base64.is_empty() {
            return Err(format!("picture {} has no bytes to check", photo.id));
        }
        let jpeg = base64_decode(&photo.jpeg_base64)?;
        let image = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
            .map_err(|e| format!("picture {} did not decode: {e}", photo.id))?
            .to_rgba8();
        let (w, h) = (image.width() as usize, image.height() as usize);
        let mut rgba = image.into_raw();
        let mut faces: Vec<Face> = detect(&large, &rgba, w, h)?;
        faces.extend(detect(&small, &rgba, w, h)?);
        let boxes: Vec<FaceBox> = merge(faces)
            .iter()
            .map(|f| FaceBox {
                x1: f.x1,
                y1: f.y1,
                x2: f.x2,
                y2: f.y2,
            })
            .collect();
        let n = blur::process(&mut rgba, w, h, &Detection::Found(boxes))?;
        let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut out = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
            .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
            .map_err(|e| format!("picture {} did not encode: {e}", photo.id))?;
        photo.jpeg_base64 = base64_encode(&out);
        photo.width = w as u32;
        photo.height = h as u32;
        // faces found now are added to any an earlier pass already blurred
        photo.faces_blurred = if was_raw { n } else { photo.faces_blurred + n };
        total += n;
    }
    capture.raw = false;
    capture.stats.faces_blurred = capture.photos.iter().map(|p| p.faces_blurred).sum();
    capture.detector = Detector {
        name: format!("{LARGE_NAME}+{NAME}"),
        model_sha256: LARGE_SHA256.into(),
    };
    capture.notes.push(
        "Faces were blurred on the desktop, by both models, before this file was kept.".into(),
    );
    Ok(Blurred {
        json: capture.json()?,
        pictures: capture.photos.len(),
        faces: total,
        was_raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lucy_room_memory::home::*;

    fn jpeg(image: &image::RgbaImage) -> Vec<u8> {
        let rgb: Vec<u8> = image.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut out = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
            .encode(
                &rgb,
                image.width(),
                image.height(),
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        out
    }
    fn photo(id: u32, bytes: &[u8]) -> Photo {
        Photo {
            id,
            t: id as f32,
            pose: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            width: 640,
            height: 480,
            faces_blurred: 0,
            jpeg_base64: base64_encode(bytes),
        }
    }
    fn capture(photos: Vec<Photo>) -> HomeCapture {
        HomeCapture {
            format: FORMAT.into(),
            id: "raw-1".into(),
            started: "2026-11-14T00:00:00Z".into(),
            ended: "2026-11-14T00:01:00Z".into(),
            device: "test".into(),
            reference_space: "local-floor".into(),
            raw: true,
            planes: vec![],
            meshes: vec![],
            path: vec![],
            stats: Stats {
                offered: photos.len() as u32,
                kept: photos.len() as u32,
                dropped: 0,
                faces_blurred: 0,
            },
            photos,
            detector: Detector::default(),
            alignment: Alignment::default(),
            notes: vec![],
        }
    }
    fn decode(b64: &str) -> image::RgbaImage {
        image::load_from_memory(&base64_decode(b64).unwrap())
            .unwrap()
            .to_rgba8()
    }
    /// The mean difference per channel inside a rectangle.
    fn changed(a: &image::RgbaImage, b: &image::RgbaImage, x: (u32, u32), y: (u32, u32)) -> f32 {
        let mut sum = 0.0;
        let mut n = 0.0;
        for yy in y.0..y.1 {
            for xx in x.0..x.1 {
                for c in 0..3 {
                    sum += (a.get_pixel(xx, yy)[c] as f32 - b.get_pixel(xx, yy)[c] as f32).abs();
                    n += 1.0;
                }
            }
        }
        sum / n
    }

    #[test]
    fn a_raw_capture_comes_out_blurred_and_marked() {
        // someone in a room: the portrait in a 640x480 frame; and a bare wall
        let face = image::load_from_memory(include_bytes!("../tests/portrait.jpg"))
            .unwrap()
            .resize(220, 220, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let mut room = image::RgbaImage::from_pixel(640, 480, image::Rgba([150, 150, 150, 255]));
        image::imageops::overlay(&mut room, &face, 360, 110);
        let wall = image::RgbaImage::from_pixel(640, 480, image::Rgba([170, 160, 150, 255]));
        let raw = capture(vec![photo(0, &jpeg(&room)), photo(1, &jpeg(&wall))]);
        let done = blur_capture(&raw.json().unwrap()).unwrap();
        assert!(done.was_raw);
        assert_eq!(done.pictures, 2);
        let out = HomeCapture::parse(&done.json).unwrap();
        assert!(!out.raw);
        assert_eq!(out.photos[0].faces_blurred, 1);
        assert_eq!(out.photos[1].faces_blurred, 0);
        assert_eq!(out.stats.faces_blurred, 1);
        assert_eq!(out.detector.model_sha256, LARGE_SHA256);
        // the face's pixels are no longer the face; the far corner of the room is as it was
        let after = decode(&out.photos[0].jpeg_base64);
        // the head sits in the portrait's upper middle: about x 420..500, y 150..250 here
        assert!(changed(&room, &after, (425, 495), (160, 240)) > 6.0);
        assert!(changed(&room, &after, (0, 200), (300, 480)) < 2.0);
        assert!(
            changed(
                &wall,
                &decode(&out.photos[1].jpeg_base64),
                (0, 640),
                (0, 480)
            ) < 2.0
        );
    }

    #[test]
    fn one_unreadable_picture_fails_the_whole_file() {
        let raw = capture(vec![photo(0, &[0xff, 0xd8, 0xff, 0x00, 0x01])]);
        assert!(blur_capture(&raw.json().unwrap()).is_err());
        let mut empty = capture(vec![photo(0, &[])]);
        empty.photos[0].jpeg_base64.clear();
        assert!(blur_capture(&empty.json().unwrap()).is_err());
    }
}
