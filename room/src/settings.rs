//! What a visitor chose in my room (roadmap/N9.md), kept in this browser's localStorage: the view,
//! the model, a hosted provider and its model, Ollama's address and model, and which notices they
//! have read. A provider key lives under its own entry, is sent only to that provider, never goes
//! into an export, and Forget removes it.

use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "lucy-room.settings.v1";
const KEY_PREFIX: &str = "lucy-room.key.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "chat" or "scene"
    pub view: String,
    /// "scripted", "words", "provider" or "ollama"
    pub backend: String,
    pub provider: String,
    pub provider_model: String,
    pub ollama_base: String,
    pub ollama_model: String,
    /// Providers whose notice the visitor accepted: their messages and my retrieved public
    /// memory go to that provider.
    pub accepted: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            view: "chat".into(),
            backend: "scripted".into(),
            provider: "openrouter".into(),
            provider_model: "openrouter/free".into(),
            ollama_base: "http://127.0.0.1:11434".into(),
            ollama_model: String::new(),
            accepted: vec![],
        }
    }
}
impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s: Settings = serde_json::from_str(text).unwrap_or_default();
        if !matches!(s.view.as_str(), "chat" | "scene") {
            s.view = "chat".into();
        }
        if !matches!(
            s.backend.as_str(),
            "scripted" | "words" | "provider" | "ollama"
        ) {
            s.backend = "scripted".into();
        }
        s
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
/// The visitor's settings, or the defaults; a `?view=scene` link opens on the scene.
pub fn load() -> Settings {
    let mut s = storage()
        .and_then(|st| st.get_item(SETTINGS_KEY).ok().flatten())
        .map(|t| Settings::parse(&t))
        .unwrap_or_default();
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&query) {
        if let Some(v) = params.get("view").filter(|v| v == "chat" || v == "scene") {
            s.view = v;
        }
    }
    s
}
pub fn save(s: &Settings) {
    if let Some(st) = storage() {
        let _ = st.set_item(SETTINGS_KEY, &s.to_json());
    }
}
pub fn key(provider: &str) -> Option<String> {
    storage()?
        .get_item(&format!("{KEY_PREFIX}{provider}"))
        .ok()
        .flatten()
        .filter(|k| !k.trim().is_empty())
}
pub fn set_key(provider: &str, key: &str) {
    if let Some(st) = storage() {
        let name = format!("{KEY_PREFIX}{provider}");
        if key.trim().is_empty() {
            let _ = st.remove_item(&name);
        } else {
            let _ = st.set_item(&name, key.trim());
        }
    }
}
/// Forget: the settings and every saved key.
pub fn forget_all() {
    let Some(st) = storage() else {
        return;
    };
    let n = st.length().unwrap_or(0);
    let mut names = vec![];
    for i in 0..n {
        if let Ok(Some(k)) = st.key(i) {
            if k == SETTINGS_KEY || k.starts_with(KEY_PREFIX) {
                names.push(k);
            }
        }
    }
    for k in names {
        let _ = st.remove_item(&k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_unknown_values_fall_back() {
        let s = Settings {
            backend: "ollama".into(),
            ollama_model: "qwen3".into(),
            ..Default::default()
        };
        assert_eq!(Settings::parse(&s.to_json()), s);
        let odd = Settings::parse(r#"{"view":"attic","backend":"oracle"}"#);
        assert_eq!(odd.view, "chat");
        assert_eq!(odd.backend, "scripted");
        assert_eq!(Settings::parse("not json"), Settings::default());
    }
    #[test]
    fn settings_hold_no_key() {
        let s = Settings::default().to_json();
        assert!(!s.to_lowercase().contains("key"), "{s}");
    }
}
