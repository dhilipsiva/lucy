//! A captured home, kept on this disk (roadmap/N8j.md, N8j.v2.md). `lucy home import FILE`
//! checks a `lucy-home-capture-v1` file and writes it under the private data root as
//! geometry, pictures and the few facts my lexicon can say. A raw file, its faces not yet
//! blurred, is refused: `lucy-home-blur` comes first. It opens no store, writes no
//! record and touches no replication log: by dhilipsiva's choice the capture never leaves
//! this machine, not even for a private backup.
use crate::store;
use lucy_room_memory::home::{FILE_BYTES, HomeCapture, base64_decode};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub const DIR: &str = "home-captures";

/// The words my lexicon has for what a headset labels; anything else stays a label.
fn word(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "wall" => Some(("wall", "Wall")),
        "floor" => Some(("floor", "Floor")),
        "shelf" => Some(("shelf", "Shelf")),
        "table" => Some(("table", "Table")),
        _ => None,
    }
}

/// One statement per labelled surface my lexicon can name, each checked by the engine's own
/// parser; a statement it refuses is left out, never bent to fit.
pub fn facts(capture: &HomeCapture) -> (Vec<String>, BTreeMap<String, usize>) {
    let mut out = vec![];
    let mut skipped = BTreeMap::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for plane in &capture.planes {
        match word(&plane.label) {
            Some((predicate, name)) => {
                let n = seen.entry(predicate).or_insert(0);
                *n += 1;
                let thing = format!("Captured{name}{n}");
                for statement in [
                    format!("{predicate}({thing})."),
                    format!("in({thing}, CapturedHome)."),
                ] {
                    if nibli_kr::parse_checked(&statement).is_ok() {
                        out.push(statement);
                    }
                }
            }
            None => *skipped.entry(plane.label.clone()).or_insert(0) += 1,
        }
    }
    (out, skipped)
}

pub fn import(root: &Path, bytes: &[u8]) -> Result<Value> {
    if bytes.len() > FILE_BYTES {
        return Err(format!(
            "a home capture is at most {} MB",
            FILE_BYTES / 1024 / 1024
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text")?;
    let capture = HomeCapture::parse(text)?;
    if capture.raw {
        return Err("this capture is raw: its faces are not blurred yet. Run `lucy-home-blur IN.json OUT.json` (room/face, --features desktop) and import the file it writes".into());
    }
    // every picture is checked before anything is written
    let mut pictures = vec![];
    for p in &capture.photos {
        if p.jpeg_base64.is_empty() {
            continue;
        }
        let jpeg = base64_decode(&p.jpeg_base64)?;
        if jpeg.len() < 3 || jpeg[..3] != [0xff, 0xd8, 0xff] {
            return Err(format!("picture {} is not a JPEG", p.id));
        }
        pictures.push((p.id, jpeg));
    }
    let sha = store::digest(bytes);
    let dir = root.join(DIR).join(&sha);
    let already = dir.join("capture.json").exists();
    let (statements, skipped) = facts(&capture);
    if !already {
        for (id, jpeg) in &pictures {
            store::save(&dir.join("photos").join(format!("{id}.jpg")), jpeg)?;
        }
        store::save(
            &dir.join("facts.nibli"),
            (statements.join("\n") + "\n").as_bytes(),
        )?;
        let labels: Vec<String> = capture
            .labels()
            .iter()
            .map(|(l, n)| format!("{n} {l}"))
            .collect();
        store::save(
            &dir.join("summary.txt"),
            format!(
                "A capture of a home, made {} to {} and imported from my room's export.\n\
                 Surfaces: {} ({}). Meshes: {}. Path points: {}.\n\
                 Pictures: {} kept of {} offered, {} dropped, {} faces blurred by {}.\n\
                 Kept on this disk only: never a store record, a replication log or a backup.\n\
                 facts.nibli holds what my lexicon can say; these labels have no word there: {}.\n",
                capture.started,
                capture.ended,
                capture.planes.len(),
                labels.join(", "),
                capture.meshes.len(),
                capture.path.len(),
                pictures.len(),
                capture.stats.offered,
                capture.stats.dropped,
                capture.stats.faces_blurred,
                capture.detector.name,
                if skipped.is_empty() {
                    "none".to_string()
                } else {
                    skipped.keys().cloned().collect::<Vec<_>>().join(", ")
                }
            )
            .as_bytes(),
        )?;
        // the geometry last: its presence marks a complete import
        store::save(
            &dir.join("capture.json"),
            capture.without_pictures().json()?.as_bytes(),
        )?;
    }
    Ok(json!({
        "imported": !already,
        "already_present": already,
        "sha256": sha,
        "directory": dir,
        "planes": capture.planes.len(),
        "meshes": capture.meshes.len(),
        "pictures": pictures.len(),
        "labels": capture.labels(),
        "facts": statements.len(),
        "labels_without_a_word": skipped,
        "stays": "on this disk only; no store record, replication log or backup carries it",
    }))
}

pub fn command(args: &[String]) -> Result<Value> {
    if args.first().map(String::as_str) != Some("import") {
        return Err("usage: lucy home import FILE [--root DIR]".into());
    }
    let file = PathBuf::from(args.get(1).ok_or("home import requires a capture file")?);
    let mut root = None;
    let mut iter = args.iter().skip(2);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--root" => root = Some(PathBuf::from(iter.next().ok_or("missing --root value")?)),
            _ => return Err(format!("unknown home option {flag}")),
        }
    }
    let root = root.map(Ok).unwrap_or_else(store::default_root)?;
    import(&root, &fs::read(&file).map_err(err)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lucy_room_memory::home::*;

    fn capture() -> HomeCapture {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let plane = |label: &str| Plane {
            label: label.into(),
            orientation: "vertical".into(),
            pose: identity,
            polygon: vec![[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0]],
        };
        HomeCapture {
            format: FORMAT.into(),
            id: "home-1".into(),
            started: "2026-11-14T00:00:00Z".into(),
            ended: "2026-11-14T00:05:00Z".into(),
            device: "test".into(),
            reference_space: "local-floor".into(),
            raw: false,
            planes: vec![
                plane("wall"),
                plane("wall"),
                plane("floor"),
                plane("ceiling"),
            ],
            meshes: vec![],
            path: vec![],
            photos: vec![Photo {
                id: 7,
                t: 1.0,
                pose: identity,
                width: 640,
                height: 480,
                faces_blurred: 1,
                jpeg_base64: base64_encode(&[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9]),
            }],
            stats: Stats {
                offered: 1,
                kept: 1,
                dropped: 0,
                faces_blurred: 1,
            },
            detector: Detector {
                name: "ultraface-rfb-320".into(),
                model_sha256: "ab".into(),
            },
            alignment: Alignment::default(),
            notes: vec![],
        }
    }
    fn tree(root: &Path) -> Vec<String> {
        let mut out = vec![];
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(
                        p.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn an_import_lands_on_this_disk_and_nowhere_else() {
        let dir = tempfile::tempdir().unwrap();
        let text = capture().json().unwrap();
        let report = import(dir.path(), text.as_bytes()).unwrap();
        assert_eq!(report["imported"], true);
        assert_eq!(report["pictures"], 1);
        let sha = report["sha256"].as_str().unwrap();
        let base = format!("{DIR}/{sha}");
        // exactly these files: no store, no records, no replication log
        assert_eq!(
            tree(dir.path()),
            vec![
                format!("{base}/capture.json"),
                format!("{base}/facts.nibli"),
                format!("{base}/photos/7.jpg"),
                format!("{base}/summary.txt"),
            ]
        );
        let home = dir.path().join(DIR).join(sha);
        let kept = fs::read_to_string(home.join("capture.json")).unwrap();
        let parsed = HomeCapture::parse(&kept).unwrap();
        assert!(
            parsed.photos[0].jpeg_base64.is_empty(),
            "pictures sit beside the geometry"
        );
        assert_eq!(
            fs::read(home.join("photos/7.jpg")).unwrap(),
            [0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in ["capture.json", "photos/7.jpg", "facts.nibli"] {
                let mode = fs::metadata(home.join(f)).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600, "{f}");
            }
            let mode = fs::metadata(&home).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
        // again: nothing new, and it says so
        let again = import(dir.path(), text.as_bytes()).unwrap();
        assert_eq!(again["already_present"], true);
        assert_eq!(tree(dir.path()).len(), 4);
    }

    #[test]
    fn facts_use_only_words_the_engine_knows() {
        let (statements, skipped) = facts(&capture());
        assert!(
            statements.contains(&"wall(CapturedWall1).".to_string()),
            "{statements:?}"
        );
        assert!(statements.contains(&"wall(CapturedWall2).".to_string()));
        assert!(statements.contains(&"floor(CapturedFloor1).".to_string()));
        for s in &statements {
            nibli_kr::parse_checked(s).unwrap();
        }
        assert_eq!(skipped.get("ceiling"), Some(&1));
        assert!(!statements.iter().any(|s| s.contains("eiling")));
    }

    #[test]
    fn a_bad_file_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(import(dir.path(), b"{}").is_err());
        let mut c = capture();
        c.photos[0].jpeg_base64 = base64_encode(b"not a jpeg");
        assert!(
            import(dir.path(), c.json().unwrap().as_bytes())
                .unwrap_err()
                .contains("not a JPEG")
        );
        let stranger = capture()
            .json()
            .unwrap()
            .replacen("\"id\"", "\"where\":\"x\",\"id\"", 1);
        assert!(import(dir.path(), stranger.as_bytes()).is_err());
        // a raw capture, faces not blurred, never enters my store
        let mut raw = capture();
        raw.raw = true;
        assert!(
            import(dir.path(), raw.json().unwrap().as_bytes())
                .unwrap_err()
                .contains("lucy-home-blur")
        );
        assert!(tree(dir.path()).is_empty());
    }
}
