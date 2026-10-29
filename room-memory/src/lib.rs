//! Exact visible messages. Provenance is reported evidence, never authentication.
pub mod transport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const FORMAT: &str = "lucy-room-memory";
pub const SOURCE: &str = "lucy-room";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub peer: String,
    pub id: String,
    pub timestamp: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Provenance {
    pub room: String,
    pub device: String,
    pub status: String,
    pub model: Option<String>,
    pub revision: Option<String>,
    pub references: Vec<Reference>,
    /// Complete original legacy Nell JSON value, including its exact message text.
    pub legacy: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub source: String,
    pub session: String,
    pub role: String,
    pub channel: String,
    pub speaker: String,
    pub text: String,
    pub timestamp: String,
    pub timestamp_kind: String,
    pub private: bool,
    pub provenance: Provenance,
}

impl Message {
    pub fn key(&self) -> String {
        serde_json::to_string(&(&self.source, &self.session, &self.id)).unwrap()
    }
    pub fn origin(&self) -> String {
        format!(
            "conversation:{}",
            serde_json::to_string(&(&self.source, &self.session)).unwrap()
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.session.trim().is_empty()
            || self.text.is_empty()
            || self.timestamp.is_empty()
            || self.speaker.is_empty()
            || !self.private
        {
            return Err("room messages require identity, text, timestamp and private scope".into());
        }
        if !matches!(self.source.as_str(), SOURCE | "nell")
            || !matches!(self.role.as_str(), "user" | "assistant")
            || !matches!(self.channel.as_str(), "user" | "final" | "summary" | "room")
            || !matches!(self.timestamp_kind.as_str(), "capture" | "source")
            || !matches!(
                self.provenance.status.as_str(),
                "complete" | "interrupted" | "error" | "legacy"
            )
        {
            return Err("unsupported room message attribution or channel".into());
        }
        if self.provenance.room.contains(['?', '#']) {
            return Err("room provenance must omit query strings and fragments".into());
        }
        if let Some(v) = &self.provenance.legacy {
            let obj = v.as_object().ok_or("legacy record must be an object")?;
            let allowed = [
                "id",
                "speaker",
                "text",
                "kind",
                "timestamp",
                "host",
                "source",
                "session",
                "channel",
                "about",
                "private",
                "from",
                "kr",
            ];
            if obj.keys().any(|k| !allowed.contains(&k.as_str()))
                || v["source"] != "nell"
                || v["id"] != self.id
                || v["text"] != self.text
                || v["timestamp"] != self.timestamp
                || v["speaker"] != self.speaker
                || v["private"] != true
            {
                return Err("invalid legacy original".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomArchiveV1 {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub messages: Vec<Message>,
}
impl Default for RoomArchiveV1 {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            exported_at: String::new(),
            messages: vec![],
        }
    }
}
impl RoomArchiveV1 {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let archive: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        archive.validate()?;
        Ok(archive)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.version != 1 {
            return Err("unsupported room archive version".into());
        }
        let mut seen = BTreeMap::new();
        for m in &self.messages {
            m.validate()?;
            if let Some(old) = seen.insert(m.key(), m) {
                if old != m {
                    return Err(format!("conflicting record {}", m.id));
                }
            }
        }
        Ok(())
    }
    /// Validate the complete batch before making any change, including on conflict.
    pub fn merge(&mut self, other: &Self) -> Result<usize, String> {
        self.validate()?;
        other.validate()?;
        let mut seen: BTreeMap<_, _> = self.messages.iter().map(|m| (m.key(), m)).collect();
        let mut incoming = vec![];
        for m in &other.messages {
            match seen.get(&m.key()) {
                Some(old) if **old != *m => return Err(format!("conflicting record {}", m.id)),
                Some(_) => (),
                None => {
                    seen.insert(m.key(), m);
                    incoming.push(m.clone());
                }
            }
        }
        let count = incoming.len();
        self.messages.extend(incoming);
        Ok(count)
    }
    pub fn json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn legacy_messages(text: &str, room: &str, device: &str) -> Result<Vec<Message>, String> {
    let originals: Vec<serde_json::Value> =
        serde_json::from_str(text).map_err(|e| e.to_string())?;
    originals
        .into_iter()
        .map(|v| {
            let s = |key: &str| v[key].as_str().unwrap_or_default().to_string();
            let role = if matches!(s("speaker").as_str(), "Nell" | "Lucy") {
                "assistant"
            } else {
                "user"
            };
            let m = Message {
                id: s("id"),
                source: s("source"),
                session: s("session"),
                role: role.into(),
                channel: s("channel"),
                speaker: s("speaker"),
                text: s("text"),
                timestamp: s("timestamp"),
                timestamp_kind: "capture".into(),
                private: true,
                provenance: Provenance {
                    room: room.into(),
                    device: device.into(),
                    status: "legacy".into(),
                    legacy: Some(v),
                    ..Default::default()
                },
            };
            m.validate()?;
            Ok(m)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(id: usize) -> Message {
        Message {
            id: format!("m-{id}"),
            source: SOURCE.into(),
            session: "session".into(),
            role: "user".into(),
            channel: "user".into(),
            speaker: "Visitor".into(),
            text: "தமிழ்\n\"hello\" \\ 🐦".into(),
            timestamp: "2026-10-26T00:00:00Z".into(),
            timestamp_kind: "capture".into(),
            private: true,
            provenance: Provenance {
                status: "complete".into(),
                ..Default::default()
            },
        }
    }
    #[test]
    fn exact_roundtrip_without_a_message_cap() {
        let a = RoomArchiveV1 {
            messages: (0..2501).map(message).collect(),
            ..Default::default()
        };
        assert_eq!(
            RoomArchiveV1::parse(a.json().unwrap().as_bytes()).unwrap(),
            a
        );
    }
    #[test]
    fn merge_is_idempotent_and_conflicts_do_not_partially_apply() {
        let mut a = RoomArchiveV1 {
            messages: vec![message(1)],
            ..Default::default()
        };
        assert_eq!(a.merge(&a.clone()).unwrap(), 0);
        let before = a.clone();
        let mut conflict = message(1);
        conflict.text = "different".into();
        assert!(a
            .merge(&RoomArchiveV1 {
                messages: vec![message(2), conflict],
                ..Default::default()
            })
            .is_err());
        assert_eq!(a, before);
    }
    #[test]
    fn refuses_other_channels_sources_and_public_records() {
        for (key, value) in [
            ("channel", "analysis"),
            ("role", "tool"),
            ("source", "codex"),
        ] {
            let mut v = serde_json::to_value(message(1)).unwrap();
            v[key] = value.into();
            assert!(serde_json::from_value::<Message>(v)
                .unwrap()
                .validate()
                .is_err());
        }
        let mut m = message(1);
        m.private = false;
        assert!(m.validate().is_err());
    }
}
