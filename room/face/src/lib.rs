//! My face finder (roadmap/N8j.md, N8j.v2.md): UltraFace through tract. Before a picture of
//! someone's home enters my store, every face in it is found and pixelated, so this errs
//! toward finding too many. In a browser it runs in a worker (the RFB-320 model), when my
//! room opens a raw capture; on the desktop, `lucy-home-blur` adds the larger RFB-640 model
//! at the camera's full size.

use sha2::{Digest, Sha256};
use tract_onnx::prelude::*;

#[cfg(feature = "desktop")]
pub mod capture;

pub const NAME: &str = "ultraface-rfb-320";
pub const MODEL_SHA256: &str = "34cd7e60aeff28744c657de7a3dc64e872d506741de66987f3426f2b79f88017";
static MODEL: &[u8] = include_bytes!("../model/version-RFB-320.onnx");
#[cfg(feature = "desktop")]
pub const LARGE_NAME: &str = "ultraface-rfb-640";
#[cfg(feature = "desktop")]
pub const LARGE_SHA256: &str = "8f4c659275977e7a3bfbfa339a9c769ad793df50f9c0baa8c14b11baa1646430";
#[cfg(feature = "desktop")]
static LARGE: &[u8] = include_bytes!("../model/version-RFB-640.onnx");
/// Lower than the model's usual 0.7: a wall blurred by mistake costs less than a face kept.
pub const THRESHOLD: f32 = 0.5;

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

/// A loaded model and the picture size it reads.
pub struct Net {
    plan: Plan,
    pub width: usize,
    pub height: usize,
}

/// A face's box in the picture, corners in 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub score: f32,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn model_digest() -> String {
    digest(MODEL)
}

fn build(bytes: &[u8], pinned: &str, width: usize, height: usize) -> Result<Net, String> {
    if digest(bytes) != pinned {
        return Err("the face model is not the one I pinned".into());
    }
    (|| -> TractResult<Plan> {
        tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(bytes))?
            .with_input_fact(0, f32::fact([1, 3, height, width]).into())?
            .into_optimized()?
            .into_runnable()
    })()
    .map(|plan| Net {
        plan,
        width,
        height,
    })
    .map_err(|e| format!("the face model did not load: {e}"))
}

/// The small model: 320×240, what a browser can carry.
pub fn load() -> Result<Net, String> {
    build(MODEL, MODEL_SHA256, 320, 240)
}
/// The larger model: 640×480, the headset camera's own size.
#[cfg(feature = "desktop")]
pub fn load_large() -> Result<Net, String> {
    build(LARGE, LARGE_SHA256, 640, 480)
}

/// Bilinear resize of an RGBA picture to a model's RGB input.
pub fn to_input(
    rgba: &[u8],
    width: usize,
    height: usize,
    w: usize,
    h: usize,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || rgba.len() != width * height * 4 {
        return Err("the picture's size does not match its bytes".into());
    }
    let mut out = vec![0u8; w * h * 3];
    for y in 0..h {
        let fy = (y as f32 + 0.5) * height as f32 / h as f32 - 0.5;
        let y0 = fy.floor().clamp(0.0, height as f32 - 1.0) as usize;
        let y1 = (y0 + 1).min(height - 1);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..w {
            let fx = (x as f32 + 0.5) * width as f32 / w as f32 - 0.5;
            let x0 = fx.floor().clamp(0.0, width as f32 - 1.0) as usize;
            let x1 = (x0 + 1).min(width - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            for c in 0..3 {
                let p = |xx: usize, yy: usize| rgba[(yy * width + xx) * 4 + c] as f32;
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out[(y * w + x) * 3 + c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
    }
    Ok(out)
}

/// Faces in an RGBA picture of any size.
pub fn detect(net: &Net, rgba: &[u8], width: usize, height: usize) -> Result<Vec<Face>, String> {
    let (w, h) = (net.width, net.height);
    let rgb = to_input(rgba, width, height, w, h)?;
    let input: Tensor = tract_ndarray::Array4::from_shape_fn((1, 3, h, w), |(_, c, y, x)| {
        (rgb[(y * w + x) * 3 + c] as f32 - 127.0) / 128.0
    })
    .into();
    (|| -> TractResult<Vec<Face>> {
        let out = net.plan.run(tvec!(input.into()))?;
        let scores = out[0].to_array_view::<f32>()?;
        let boxes = out[1].to_array_view::<f32>()?;
        let mut found = vec![];
        for i in 0..scores.shape()[1] {
            let score = scores[[0, i, 1]];
            if score >= THRESHOLD {
                found.push(Face {
                    x1: boxes[[0, i, 0]].clamp(0.0, 1.0),
                    y1: boxes[[0, i, 1]].clamp(0.0, 1.0),
                    x2: boxes[[0, i, 2]].clamp(0.0, 1.0),
                    y2: boxes[[0, i, 3]].clamp(0.0, 1.0),
                    score,
                });
            }
        }
        Ok(merge(found))
    })()
    .map_err(|e| format!("the face model did not run: {e}"))
}

/// Keep the surest of any boxes that overlap.
pub fn merge(mut found: Vec<Face>) -> Vec<Face> {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Face> = vec![];
    for f in found {
        if keep.iter().all(|k| overlap(k, &f) < 0.3) {
            keep.push(f);
        }
    }
    keep
}

fn overlap(a: &Face, b: &Face) -> f32 {
    let w = (a.x2.min(b.x2) - a.x1.max(b.x1)).max(0.0);
    let h = (a.y2.min(b.y2) - a.y1.max(b.y1)).max(0.0);
    let i = w * h;
    i / ((a.x2 - a.x1) * (a.y2 - a.y1) + (b.x2 - b.x1) * (b.y2 - b.y1) - i).max(1e-6)
}

/// The worker: a picture in, face boxes out. `{id, width, height, rgba}` →
/// `{id, ok: true, boxes: Float32Array[x1, y1, x2, y2, score, …]}` or `{id, ok: false, error}`.
#[cfg(target_arch = "wasm32")]
mod worker {
    use super::*;
    use js_sys::{Float32Array, Object, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    fn set(o: &Object, k: &str, v: &JsValue) {
        let _ = Reflect::set(o, &k.into(), v);
    }

    #[wasm_bindgen]
    pub fn worker_main() {
        console_error_panic_hook::set_once();
        let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
        let hello = Object::new();
        let net = match load() {
            Ok(n) => {
                set(&hello, "ready", &JsValue::TRUE);
                set(&hello, "name", &NAME.into());
                set(&hello, "model_sha256", &MODEL_SHA256.into());
                Some(n)
            }
            Err(e) => {
                set(&hello, "fatal", &e.into());
                None
            }
        };
        let _ = scope.post_message(&hello);
        let Some(net) = net else { return };
        let out = scope.clone();
        let on = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let data = e.data();
            let get = |k: &str| Reflect::get(&data, &k.into()).unwrap_or(JsValue::UNDEFINED);
            let reply = Object::new();
            set(&reply, "id", &get("id"));
            let width = get("width").as_f64().unwrap_or(0.0) as usize;
            let height = get("height").as_f64().unwrap_or(0.0) as usize;
            let rgba = Uint8Array::new(&get("rgba")).to_vec();
            match detect(&net, &rgba, width, height) {
                Ok(faces) => {
                    let flat: Vec<f32> = faces
                        .iter()
                        .flat_map(|f| [f.x1, f.y1, f.x2, f.y2, f.score])
                        .collect();
                    set(&reply, "ok", &JsValue::TRUE);
                    set(&reply, "boxes", &Float32Array::from(flat.as_slice()));
                }
                Err(err) => {
                    set(&reply, "ok", &JsValue::FALSE);
                    set(&reply, "error", &err.into());
                }
            }
            let _ = out.post_message(&reply);
        });
        scope.set_onmessage(Some(on.as_ref().unchecked_ref()));
        on.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portrait() -> (Vec<u8>, usize, usize) {
        let img = image::load_from_memory(include_bytes!("../tests/portrait.jpg"))
            .unwrap()
            .to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        (img.into_raw(), w, h)
    }

    #[test]
    fn the_model_is_the_pinned_one() {
        assert_eq!(model_digest(), MODEL_SHA256);
    }

    #[test]
    fn finds_the_portrait_and_not_a_wall() {
        let net = load().unwrap();
        let (rgba, w, h) = portrait();
        let faces = detect(&net, &rgba, w, h).unwrap();
        assert_eq!(faces.len(), 1, "{faces:?}");
        let f = faces[0];
        // the face sits in the upper middle of the portrait
        assert!(
            f.x1 < 0.5 && f.x2 > 0.5 && f.y1 < 0.45 && f.y2 > 0.2,
            "{f:?}"
        );
        let wall = vec![180u8; 640 * 480 * 4];
        assert!(detect(&net, &wall, 640, 480).unwrap().is_empty());
    }

    #[test]
    fn a_small_face_in_a_room_sized_picture_is_still_found() {
        // the portrait pasted at a third of the height of a 640x480 grey frame
        let net = load().unwrap();
        let small = image::load_from_memory(include_bytes!("../tests/portrait.jpg"))
            .unwrap()
            .resize(200, 200, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let mut frame = image::RgbaImage::from_pixel(640, 480, image::Rgba([150, 150, 150, 255]));
        image::imageops::overlay(&mut frame, &small, 380, 120);
        let faces = detect(&net, frame.as_raw(), 640, 480).unwrap();
        assert!(!faces.is_empty());
        assert!(faces.iter().any(|f| f.x1 > 0.5 && f.y1 > 0.2), "{faces:?}");
    }

    #[test]
    fn a_wrong_size_is_an_error_not_a_guess() {
        let net = load().unwrap();
        assert!(detect(&net, &[0u8; 10], 640, 480).is_err());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn the_larger_model_finds_a_face_too_small_to_be_sure_of() {
        // the portrait at 70 pixels high in a 640x480 frame: a person across a room
        let small = image::load_from_memory(include_bytes!("../tests/portrait.jpg"))
            .unwrap()
            .resize(70, 70, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let mut frame = image::RgbaImage::from_pixel(640, 480, image::Rgba([150, 150, 150, 255]));
        image::imageops::overlay(&mut frame, &small, 100, 200);
        let large = load_large().unwrap();
        assert_eq!((large.width, large.height), (640, 480));
        let faces = detect(&large, frame.as_raw(), 640, 480).unwrap();
        assert!(
            faces
                .iter()
                .any(|f| f.x1 < 0.3 && f.y1 > 0.35 && f.y2 < 0.6),
            "{faces:?}"
        );
        let wall = vec![180u8; 640 * 480 * 4];
        assert!(detect(&large, &wall, 640, 480).unwrap().is_empty());
    }
}
