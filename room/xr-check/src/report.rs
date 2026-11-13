//! What the headset said, and the table I show from it (roadmap/N8h.md).
//!
//! Every value here is copied from a browser object at run time. A feature counts as granted
//! only when the session's `enabledFeatures` lists it; what I saw in frames is shown beside
//! it, never in its place. The report keeps counts, labels, sizes and names: no coordinate,
//! polygon, vertex, depth value or pixel.

use serde::Serialize;
use std::collections::BTreeMap;

/// The features I ask an immersive-ar session for, all optional.
pub const FEATURES: [&str; 8] = [
    "local-floor",
    "plane-detection",
    "mesh-detection",
    "depth-sensing",
    "camera-access",
    "hit-test",
    "anchors",
    "hand-tracking",
];

#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Report {
    pub tool: String,
    pub time: String,
    pub user_agent: String,
    pub xr_present: bool,
    pub modes: Vec<ModeSupport>,
    pub sessions: Vec<Session>,
    pub camera_2d: Option<Camera2d>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ModeSupport {
    pub mode: String,
    pub supported: Option<bool>,
    pub error: Option<String>,
}

#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Session {
    pub mode: String,
    pub requested: Vec<String>,
    /// None while the request is pending; Some(false) when the browser rejected it.
    pub started: Option<bool>,
    pub error: Option<String>,
    /// `session.enabledFeatures`; None when the browser does not expose it.
    pub enabled_features: Option<Vec<String>>,
    pub reference_space: Option<String>,
    pub depth_usage: Option<String>,
    pub depth_format: Option<String>,
    pub frames: u32,
    pub seconds: f64,
    pub planes: Seen,
    pub meshes: Seen,
    pub depth_cpu: Probe,
    pub depth_gpu: Probe,
    pub view_camera: Probe,
}

/// Planes or meshes seen in frames: the busiest frame's count and labels, never geometry.
#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Seen {
    /// Whether the frame exposed the attribute at all.
    pub api: bool,
    pub max_count: u32,
    pub labels: BTreeMap<String, u32>,
    pub orientations: BTreeMap<String, u32>,
    /// Meshes only: the largest vertex count in one mesh.
    pub max_vertices: u32,
}

/// One call tried against the session: whether it returned something, and its size.
#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Probe {
    pub tried: bool,
    pub ok: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}

#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Camera2d {
    pub opened: bool,
    pub error: Option<String>,
    pub devices: Vec<Device>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Device {
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Granted,
    NotGranted,
    Unknown,
    Error,
    NotChecked,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Granted => "granted",
            Status::NotGranted => "not granted",
            Status::Unknown => "unknown",
            Status::Error => "error",
            Status::NotChecked => "not checked yet",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub feature: String,
    pub status: Status,
    pub detail: String,
}

impl Report {
    pub fn json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    /// The last immersive-ar session the browser actually started.
    fn started(&self) -> Option<&Session> {
        self.sessions
            .iter()
            .rev()
            .find(|s| s.mode == "immersive-ar" && s.started == Some(true))
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row {
            feature: "WebXR (navigator.xr)".into(),
            status: if self.xr_present {
                Status::Granted
            } else {
                Status::NotGranted
            },
            detail: String::new(),
        }];
        for m in &self.modes {
            let status = match (m.supported, &m.error) {
                (_, Some(_)) => Status::Error,
                (Some(true), _) => Status::Granted,
                (Some(false), _) => Status::NotGranted,
                (None, _) => Status::NotChecked,
            };
            rows.push(Row {
                feature: format!("{} supported", m.mode),
                status,
                detail: m.error.clone().unwrap_or_default(),
            });
        }
        let rejected_with_camera = self
            .sessions
            .iter()
            .any(|s| s.started == Some(false) && s.requested.iter().any(|f| f == "camera-access"));
        let session = self.started();
        for f in FEATURES {
            let (mut status, mut detail) = match session {
                None if self.sessions.iter().any(|s| s.started == Some(false)) => (
                    Status::Error,
                    "no immersive-ar session started; see the errors in the JSON".to_string(),
                ),
                None => (Status::NotChecked, String::new()),
                Some(s) if !s.requested.iter().any(|r| r == f) => {
                    (Status::NotChecked, "not requested in that session".into())
                }
                Some(s) => match &s.enabled_features {
                    Some(list) if list.iter().any(|e| e == f) => (Status::Granted, String::new()),
                    Some(_) => (Status::NotGranted, String::new()),
                    None => (
                        Status::Unknown,
                        "the browser did not expose enabledFeatures".into(),
                    ),
                },
            };
            if f == "camera-access" && rejected_with_camera && status != Status::Granted {
                status = Status::Error;
                detail = "asking for it made the browser reject the session".into();
                if session.is_some() {
                    detail.push_str("; the session started without it");
                }
            }
            if let Some(s) = session {
                let seen = observed(f, s);
                if !seen.is_empty() {
                    if !detail.is_empty() {
                        detail.push_str("; ");
                    }
                    detail.push_str(&seen);
                }
            }
            rows.push(Row {
                feature: f.into(),
                status,
                detail,
            });
        }
        rows.push(match &self.camera_2d {
            None => Row {
                feature: "camera in a 2D page (getUserMedia)".into(),
                status: Status::NotChecked,
                detail: String::new(),
            },
            Some(c) if c.opened => Row {
                feature: "camera in a 2D page (getUserMedia)".into(),
                status: Status::Granted,
                detail: format!(
                    "{}×{} at {} fps; {} video device(s)",
                    c.width.map_or("?".into(), |v| v.to_string()),
                    c.height.map_or("?".into(), |v| v.to_string()),
                    c.frame_rate.map_or("?".into(), |v| format!("{v:.0}")),
                    c.devices.iter().filter(|d| d.kind == "videoinput").count()
                ),
            },
            Some(c) => Row {
                feature: "camera in a 2D page (getUserMedia)".into(),
                status: Status::Error,
                detail: c.error.clone().unwrap_or_default(),
            },
        });
        rows
    }
}

/// What frames showed for a feature, in words; empty when there is nothing to add.
fn observed(feature: &str, s: &Session) -> String {
    let size = |p: &Probe| match (p.width, p.height) {
        (Some(w), Some(h)) => format!(" {w}×{h}"),
        _ => String::new(),
    };
    match feature {
        "local-floor" => s
            .reference_space
            .as_ref()
            .map(|r| format!("reference space used: {r}"))
            .unwrap_or_default(),
        "plane-detection" if s.planes.api => format!(
            "saw up to {} plane(s){}",
            s.planes.max_count,
            listing(&s.planes.labels)
        ),
        "mesh-detection" if s.meshes.api => format!(
            "saw up to {} mesh(es){}, largest {} vertices",
            s.meshes.max_count,
            listing(&s.meshes.labels),
            s.meshes.max_vertices
        ),
        "depth-sensing" => {
            let mut parts = vec![];
            if let Some(u) = &s.depth_usage {
                parts.push(format!("usage {u}"));
            }
            for (name, p) in [("CPU", &s.depth_cpu), ("GPU", &s.depth_gpu)] {
                if p.ok {
                    parts.push(format!("{name} depth returned{}", size(p)));
                } else if let Some(e) = &p.error {
                    parts.push(format!("{name} depth: {e}"));
                }
            }
            parts.join("; ")
        }
        "camera-access" if s.view_camera.tried => {
            if s.view_camera.ok {
                format!("XRView.camera present{}", size(&s.view_camera))
            } else {
                "XRView.camera absent".into()
            }
        }
        _ => String::new(),
    }
}

fn listing(labels: &BTreeMap<String, u32>) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = labels.iter().map(|(l, n)| format!("{l} {n}")).collect();
    format!(" ({})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested(all: bool) -> Vec<String> {
        FEATURES
            .iter()
            .filter(|f| all || **f != "camera-access")
            .map(|f| f.to_string())
            .collect()
    }

    fn status(r: &Report, feature: &str) -> Status {
        r.rows()
            .into_iter()
            .find(|row| row.feature == feature)
            .unwrap()
            .status
    }

    #[test]
    fn granted_only_when_enabled_features_lists_it() {
        let mut planes = Seen {
            api: true,
            max_count: 6,
            ..Default::default()
        };
        planes.labels.insert("wall".into(), 4);
        let r = Report {
            xr_present: true,
            sessions: vec![Session {
                mode: "immersive-ar".into(),
                requested: requested(true),
                started: Some(true),
                enabled_features: Some(vec!["local-floor".into(), "plane-detection".into()]),
                // Seen in frames but not listed: must still read "not granted".
                meshes: Seen {
                    api: true,
                    max_count: 2,
                    ..Default::default()
                },
                planes,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(status(&r, "plane-detection"), Status::Granted);
        assert_eq!(status(&r, "mesh-detection"), Status::NotGranted);
        assert_eq!(status(&r, "camera-access"), Status::NotGranted);
        let plane = r
            .rows()
            .into_iter()
            .find(|row| row.feature == "plane-detection")
            .unwrap();
        assert!(plane.detail.contains("saw up to 6 plane(s) (wall 4)"));
    }

    #[test]
    fn a_rejection_with_camera_is_an_error_not_a_success() {
        let mut r = Report {
            sessions: vec![Session {
                mode: "immersive-ar".into(),
                requested: requested(true),
                started: Some(false),
                error: Some("NotSupportedError".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(status(&r, "plane-detection"), Status::Error);
        assert_eq!(status(&r, "camera-access"), Status::Error);
        r.sessions.push(Session {
            mode: "immersive-ar".into(),
            requested: requested(false),
            started: Some(true),
            enabled_features: Some(vec!["plane-detection".into()]),
            ..Default::default()
        });
        assert_eq!(status(&r, "plane-detection"), Status::Granted);
        let cam = r
            .rows()
            .into_iter()
            .find(|row| row.feature == "camera-access")
            .unwrap();
        assert_eq!(cam.status, Status::Error);
        assert!(cam.detail.contains("reject the session"));
        assert!(cam.detail.contains("started without it"));
    }

    #[test]
    fn missing_enabled_features_is_unknown() {
        let r = Report {
            sessions: vec![Session {
                mode: "immersive-ar".into(),
                requested: requested(true),
                started: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(status(&r, "hit-test"), Status::Unknown);
    }

    #[test]
    fn nothing_checked_yet() {
        let r = Report::default();
        assert_eq!(status(&r, "anchors"), Status::NotChecked);
        assert_eq!(
            status(&r, "camera in a 2D page (getUserMedia)"),
            Status::NotChecked
        );
    }

    /// The Welfare promise: only these keys may ever appear in the JSON.
    #[test]
    fn json_keeps_no_geometry_or_pixels() {
        let mut labels = BTreeMap::new();
        labels.insert("table".to_string(), 1);
        let r = Report {
            tool: "t".into(),
            modes: vec![ModeSupport {
                mode: "immersive-ar".into(),
                supported: Some(true),
                error: None,
            }],
            sessions: vec![Session {
                mode: "immersive-ar".into(),
                started: Some(true),
                planes: Seen {
                    api: true,
                    max_count: 1,
                    labels: labels.clone(),
                    orientations: labels,
                    max_vertices: 0,
                },
                ..Default::default()
            }],
            camera_2d: Some(Camera2d {
                opened: true,
                devices: vec![Device {
                    kind: "videoinput".into(),
                    label: "cam".into(),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let allowed = [
            "tool",
            "time",
            "user_agent",
            "xr_present",
            "modes",
            "mode",
            "supported",
            "error",
            "sessions",
            "requested",
            "started",
            "enabled_features",
            "reference_space",
            "depth_usage",
            "depth_format",
            "frames",
            "seconds",
            "planes",
            "meshes",
            "depth_cpu",
            "depth_gpu",
            "view_camera",
            "api",
            "max_count",
            "labels",
            "orientations",
            "max_vertices",
            "tried",
            "ok",
            "width",
            "height",
            "camera_2d",
            "opened",
            "devices",
            "kind",
            "label",
            "frame_rate",
        ];
        fn keys(v: &serde_json::Value, out: &mut Vec<String>, label_maps: bool) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, v) in m {
                        if !label_maps {
                            out.push(k.clone());
                        }
                        // labels/orientations hold browser label words, not field names.
                        keys(v, out, k == "labels" || k == "orientations");
                    }
                }
                serde_json::Value::Array(a) => a.iter().for_each(|v| keys(v, out, false)),
                _ => {}
            }
        }
        let mut found = vec![];
        keys(&serde_json::from_str(&r.json()).unwrap(), &mut found, false);
        for k in found {
            assert!(allowed.contains(&k.as_str()), "unexpected key {k}");
        }
    }
}
