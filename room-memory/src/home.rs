//! A captured home (roadmap/N8j.md): what a headset granted while its wearer walked their
//! rooms. Planes, meshes, the walked path and pictures with the head pose of each, in one JSON
//! file the room exports and both the room and the CLI read. Private by nature: nothing here
//! is ever published, and the CLI keeps it on one disk.

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "lucy-home-capture-v1";
/// The whole file, pictures included.
pub const FILE_BYTES: usize = 320 * 1024 * 1024;
/// Every frame of a walk at about one a second: twenty-five minutes.
pub const MAX_PHOTOS: usize = 1500;
pub const MAX_PLANES: usize = 512;
pub const MAX_MESHES: usize = 64;
pub const MAX_VERTICES: usize = 400_000;
/// Thirty points a second for half an hour.
pub const MAX_PATH: usize = 60_000;
pub const PHOTO_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HomeCapture {
    pub format: String,
    pub id: String,
    pub started: String,
    pub ended: String,
    pub device: String,
    pub reference_space: String,
    /// True while the pictures are as the camera gave them: faces not yet blurred. The headset
    /// writes true (roadmap/N8j.v2.md); only a blur that ran over every picture writes false.
    pub raw: bool,
    pub planes: Vec<Plane>,
    pub meshes: Vec<SurfaceMesh>,
    pub path: Vec<PathPoint>,
    pub photos: Vec<Photo>,
    pub stats: Stats,
    pub detector: Detector,
    pub alignment: Alignment,
    pub notes: Vec<String>,
}

/// A detected plane. `pose` takes plane space to the floor-level reference space, column
/// major; `polygon` is in plane space, where +Y is the plane's normal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plane {
    pub label: String,
    pub orientation: String,
    pub pose: [f32; 16],
    pub polygon: Vec<[f32; 3]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SurfaceMesh {
    pub label: String,
    pub pose: [f32; 16],
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PathPoint {
    /// Seconds since the capture began.
    pub t: f32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Photo {
    pub id: u32,
    pub t: f32,
    /// The head's pose when the camera delivered this frame.
    pub pose: [f32; 16],
    pub width: u32,
    pub height: u32,
    pub faces_blurred: u32,
    /// The JPEG, base64. Empty where pictures are stored beside the geometry instead.
    pub jpeg_base64: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    pub offered: u32,
    pub kept: u32,
    pub dropped: u32,
    pub faces_blurred: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Detector {
    pub name: String,
    pub model_sha256: String,
}

/// Where the camera sits against the head and how wide it sees: unpublished for the headset,
/// so a starting guess the viewer's alignment panel adjusts. One setting for every picture.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    pub hfov_deg: f32,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub roll_deg: f32,
    pub dx: f32,
    pub dy: f32,
    pub dz: f32,
    /// Seconds the picture trails the pose recorded with it.
    pub lag_s: f32,
}
impl Default for Alignment {
    fn default() -> Self {
        Alignment {
            hfov_deg: 80.0,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            roll_deg: 0.0,
            dx: -0.032,
            dy: -0.02,
            dz: -0.06,
            lag_s: 0.0,
        }
    }
}

impl HomeCapture {
    pub fn json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
    /// Strict: an unknown field, a wrong format name or anything past the caps is refused.
    pub fn parse(text: &str) -> Result<HomeCapture, String> {
        if text.len() > FILE_BYTES {
            return Err(format!(
                "a home capture is at most {} MB",
                FILE_BYTES / 1024 / 1024
            ));
        }
        let c: HomeCapture =
            serde_json::from_str(text).map_err(|e| format!("not a home capture: {e}"))?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("format is {:?}, not {FORMAT:?}", self.format));
        }
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("the capture id must be letters, digits and dashes".into());
        }
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        if self.planes.len() > MAX_PLANES
            || self.meshes.len() > MAX_MESHES
            || self.photos.len() > MAX_PHOTOS
            || self.path.len() > MAX_PATH
        {
            return Err(
                "the capture has more planes, meshes, pictures or path than allowed".into(),
            );
        }
        for p in &self.planes {
            if !finite(&p.pose) || p.polygon.iter().any(|v| !finite(v)) || p.polygon.len() > 512 {
                return Err("a plane has a bad pose or polygon".into());
            }
        }
        let mut vertices = 0;
        for m in &self.meshes {
            let n = m.vertices.len() / 3;
            vertices += n;
            if !finite(&m.pose)
                || !finite(&m.vertices)
                || !m.vertices.len().is_multiple_of(3)
                || !m.indices.len().is_multiple_of(3)
                || m.indices.iter().any(|i| *i as usize >= n)
            {
                return Err("a mesh has bad vertices or indices".into());
            }
        }
        if vertices > MAX_VERTICES {
            return Err("the meshes have more vertices than allowed".into());
        }
        if self
            .path
            .iter()
            .any(|p| !p.t.is_finite() || !finite(&p.pos) || !finite(&p.quat))
        {
            return Err("the path has a bad point".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for p in &self.photos {
            if !ids.insert(p.id) {
                return Err(format!("picture {} appears twice", p.id));
            }
            if !finite(&p.pose) || !p.t.is_finite() || p.width == 0 || p.height == 0 {
                return Err(format!("picture {} has a bad pose or size", p.id));
            }
            if p.jpeg_base64.len() > PHOTO_BYTES * 4 / 3 + 4 {
                return Err(format!("picture {} is too large", p.id));
            }
        }
        let a = &self.alignment;
        if !finite(&[
            a.hfov_deg,
            a.yaw_deg,
            a.pitch_deg,
            a.roll_deg,
            a.dx,
            a.dy,
            a.dz,
            a.lag_s,
        ]) || !(20.0..=160.0).contains(&a.hfov_deg)
        {
            return Err("the alignment is out of range".into());
        }
        Ok(())
    }
    /// The same capture with every picture's bytes removed: what is kept beside the pictures.
    pub fn without_pictures(&self) -> HomeCapture {
        let mut c = self.clone();
        for p in &mut c.photos {
            p.jpeg_base64.clear();
        }
        c
    }
    /// Counts by label, for a summary that says nothing of where things are.
    pub fn labels(&self) -> std::collections::BTreeMap<String, usize> {
        let mut out = std::collections::BTreeMap::new();
        for p in &self.planes {
            *out.entry(p.label.clone()).or_insert(0) += 1;
        }
        out
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length is not a multiple of four".into());
    }
    let value = |c: u8| -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a') as u32 + 26),
            b'0'..=b'9' => Ok((c - b'0') as u32 + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("not base64".into()),
        }
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let last = i + 1 == bytes.len() / 4;
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return Err("misplaced base64 padding".into());
        }
        let mut n = 0u32;
        for c in &chunk[..4 - pad] {
            n = n << 6 | value(*c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) fn sample() -> HomeCapture {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    HomeCapture {
        format: FORMAT.into(),
        id: "test-1".into(),
        started: "2026-11-14T00:00:00Z".into(),
        ended: "2026-11-14T00:01:00Z".into(),
        device: "test".into(),
        reference_space: "local-floor".into(),
        raw: false,
        planes: vec![Plane {
            label: "wall".into(),
            orientation: "vertical".into(),
            pose: identity,
            polygon: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
                [-1.0, 0.0, 1.0],
            ],
        }],
        meshes: vec![SurfaceMesh {
            label: "global mesh".into(),
            pose: identity,
            vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            indices: vec![0, 1, 2],
        }],
        path: vec![PathPoint {
            t: 0.0,
            pos: [0.0, 1.6, 0.0],
            quat: [0.0, 0.0, 0.0, 1.0],
        }],
        photos: vec![Photo {
            id: 0,
            t: 0.5,
            pose: identity,
            width: 640,
            height: 480,
            faces_blurred: 1,
            jpeg_base64: base64_encode(&[0xff, 0xd8, 0xff, 0xd9]),
        }],
        stats: Stats {
            offered: 2,
            kept: 1,
            dropped: 1,
            faces_blurred: 1,
        },
        detector: Detector {
            name: "ultraface-rfb-320".into(),
            model_sha256: "00".into(),
        },
        alignment: Alignment::default(),
        notes: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_strangers() {
        let c = sample();
        let text = c.json().unwrap();
        assert_eq!(HomeCapture::parse(&text).unwrap(), c);
        // an unknown field anywhere is refused
        let extra = text.replacen("\"id\"", "\"gps\":[1,2],\"id\"", 1);
        assert!(HomeCapture::parse(&extra).is_err());
        let other = text.replace(FORMAT, "lucy-home-capture-v2");
        assert!(HomeCapture::parse(&other).is_err());
    }

    #[test]
    fn caps_and_bad_geometry_are_refused() {
        let mut c = sample();
        c.meshes[0].indices = vec![0, 1, 9];
        assert!(c.validate().is_err());
        let mut c = sample();
        c.photos.push(c.photos[0].clone());
        assert!(c.validate().unwrap_err().contains("twice"));
        let mut c = sample();
        c.planes[0].pose[3] = f32::NAN;
        assert!(c.validate().is_err());
        let mut c = sample();
        c.id = "../etc".into();
        assert!(c.validate().is_err());
        let mut c = sample();
        c.alignment.hfov_deg = 400.0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn pictures_can_be_set_aside() {
        let c = sample();
        let bare = c.without_pictures();
        assert!(bare.photos[0].jpeg_base64.is_empty());
        assert_eq!(bare.photos[0].pose, c.photos[0].pose);
        assert_eq!(c.labels().get("wall"), Some(&1));
        bare.validate().unwrap();
    }

    #[test]
    fn base64_both_ways() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(base64_decode(&base64_encode(bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert!(base64_decode("Zm9v!mFy").is_err());
        assert!(base64_decode("Zm8=Zm8=").is_err());
        assert!(base64_decode("abc").is_err());
    }
}
