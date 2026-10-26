//! Where my two files come from (roadmap/N2b.md): fresh from my origin's `main` over GitHub's
//! raw file service first, then the copy published beside the room. `?constitution=URL` or
//! `?memory=URL` names one source only; `?fresh=0` keeps to the published copy.

const RAW: &str = "https://raw.githubusercontent.com";
pub const ORIGIN_REPOSITORY: &str = "dhilipsiva/lucy";
pub const ORIGIN_REF: &str = "main";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub url: String,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub constitution: Vec<Candidate>,
    pub memory: Vec<Candidate>,
}

impl Sources {
    /// From the page's query string (without the leading `?`), with `../lucy` as the local copy.
    pub fn from_query(query: &str) -> Sources {
        Self::from_query_with_local(query, "../lucy")
    }
    pub fn from_query_with_local(query: &str, local: &str) -> Sources {
        let params: Vec<(String, String)> = query
            .trim_start_matches('?')
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (k.to_string(), percent_decode(v)),
                None => (p.to_string(), String::new()),
            })
            .collect();
        let get = |k: &str| {
            params
                .iter()
                .find(|(kk, _)| kk == k)
                .map(|(_, v)| v.clone())
        };
        let fresh = get("fresh").map(|v| v != "0").unwrap_or(true);
        let mut out = Sources::default();
        for name in ["constitution", "memory"] {
            let list = if let Some(given) = get(name) {
                vec![Candidate {
                    url: given,
                    label: "given on the address".into(),
                }]
            } else {
                let fresh_c = Candidate {
                    url: format!("{RAW}/{ORIGIN_REPOSITORY}/{ORIGIN_REF}/lucy/{name}.nibli"),
                    label: format!("fresh, origin {ORIGIN_REF}"),
                };
                let published = Candidate {
                    url: format!("{local}/{name}.nibli"),
                    label: "the copy published with this room".into(),
                };
                if fresh {
                    vec![fresh_c, published]
                } else {
                    vec![published]
                }
            };
            if name == "constitution" {
                out.constitution = list;
            } else {
                out.memory = list;
            }
        }
        out
    }
}
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Fetch a text over the network, no store, as the wake protocol reads files.
pub async fn fetch_text(url: &str) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or("no window")?;
    let init = web_sys::RequestInit::new();
    init.set_cache(web_sys::RequestCache::NoStore);
    let request =
        web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let resp = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| js_error(&e))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "not a response".to_string())?;
    if !resp.ok() {
        return Err(format!("{} {}", resp.status(), resp.status_text()));
    }
    let text = JsFuture::from(resp.text().map_err(|e| js_error(&e))?)
        .await
        .map_err(|e| js_error(&e))?;
    text.as_string()
        .ok_or_else(|| "the response was not text".to_string())
}
pub fn js_error(e: &wasm_bindgen::JsValue) -> String {
    e.as_string()
        .or_else(|| {
            js_sys::Reflect::get(e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

/// Resolve against the page directory, including extensionless /room links.
pub fn worker_url(role: &str) -> Result<String, String> {
    let location = web_sys::window().ok_or("no window")?.location();
    let path = location.pathname().map_err(|e| js_error(&e))?;
    let directory = if path.ends_with('/') {
        path
    } else if path.ends_with(".html") {
        path.rsplit_once('/')
            .map(|(p, _)| format!("{p}/"))
            .unwrap_or_else(|| "/".into())
    } else {
        format!("{path}/")
    };
    Ok(format!(
        "{}{directory}worker.js?role={role}",
        location.origin().map_err(|e| js_error(&e))?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_first_then_the_published_copy() {
        let s = Sources::from_query("");
        assert_eq!(s.constitution.len(), 2);
        assert!(s.constitution[0].url.starts_with(
            "https://raw.githubusercontent.com/dhilipsiva/lucy/main/lucy/constitution.nibli"
        ));
        assert_eq!(s.memory[1].url, "../lucy/memory.nibli");
    }
    #[test]
    fn fresh_zero_keeps_to_the_copy() {
        let s = Sources::from_query("?fresh=0");
        assert_eq!(s.memory.len(), 1);
        assert_eq!(s.memory[0].label, "the copy published with this room");
    }
    #[test]
    fn a_given_address_is_the_only_source() {
        let s = Sources::from_query("constitution=http%3A%2F%2Flocalhost%2Fc.nibli&fresh=0");
        assert_eq!(
            s.constitution,
            vec![Candidate {
                url: "http://localhost/c.nibli".into(),
                label: "given on the address".into()
            }]
        );
        assert_eq!(s.memory.len(), 1);
    }
}
