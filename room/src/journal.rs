//! IndexedDB originals, independent of disposable model and retrieval caches.
pub use lucy_room_memory::{Message, Provenance, RoomArchiveV1};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};

pub const DB: &str = "lucy-room-memory-v1";
fn error(e: JsValue) -> String {
    crate::sources::js_error(&e)
}
pub fn timestamp() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}
pub fn uuid() -> String {
    web_sys::window()
        .expect("window")
        .crypto()
        .expect("secure random generator")
        .random_uuid()
}
pub fn room_url() -> String {
    let location = web_sys::window().unwrap().location();
    format!(
        "{}{}",
        location.origin().unwrap_or_default(),
        location.pathname().unwrap_or_default()
    )
}
pub fn device() -> String {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    if let Some(s) = &storage {
        if let Ok(Some(id)) = s.get_item("nell.device.v1") {
            return id;
        }
    }
    let id = uuid();
    if let Some(s) = storage {
        let _ = s.set_item("nell.device.v1", &id);
    }
    id
}

async fn request(req: &IdbRequest) -> Result<JsValue, String> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let r = req.clone();
        let reject_result = reject.clone();
        let ok = Closure::once_into_js(move |_: web_sys::Event| {
            r.set_onsuccess(None);
            r.set_onerror(None);
            match r.result() {
                Ok(v) => {
                    let _ = resolve.call1(&JsValue::NULL, &v);
                }
                Err(e) => {
                    let _ = reject_result.call1(&JsValue::NULL, &e);
                }
            }
        });
        let r = req.clone();
        let fail = Closure::once_into_js(move |_: web_sys::Event| {
            r.set_onsuccess(None);
            r.set_onerror(None);
            let msg = r
                .error()
                .ok()
                .flatten()
                .map(|e| e.message())
                .unwrap_or_else(|| "IndexedDB request failed".into());
            let _ = reject.call1(&JsValue::NULL, &msg.into());
        });
        req.set_onsuccess(Some(ok.unchecked_ref()));
        req.set_onerror(Some(fail.unchecked_ref()));
    });
    JsFuture::from(promise).await.map_err(error)
}
async fn open() -> Result<IdbDatabase, String> {
    let factory = web_sys::window()
        .ok_or("no window")?
        .indexed_db()
        .map_err(error)?
        .ok_or("IndexedDB unavailable")?;
    let req = factory.open_with_u32(DB, 1).map_err(error)?;
    let copy = req.clone();
    let upgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let Ok(v) = copy.result() {
            if let Ok(db) = v.dyn_into::<IdbDatabase>() {
                for name in ["messages", "drafts", "cache"] {
                    if !db.object_store_names().contains(name) {
                        let _ = db.create_object_store(name);
                    }
                }
            }
        }
    });
    req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let v = request(req.unchecked_ref()).await;
    req.set_onupgradeneeded(None);
    v?.dyn_into()
        .map_err(|_| "not an IndexedDB database".into())
}
fn transaction_done(tx: &web_sys::IdbTransaction) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        let t = tx.clone();
        let cb = Closure::once_into_js(move |_: web_sys::Event| {
            t.set_oncomplete(None);
            t.set_onabort(None);
            t.set_onerror(None);
            let _ = resolve.call0(&JsValue::NULL);
        });
        let t = tx.clone();
        let err = Closure::once_into_js(move |_: web_sys::Event| {
            t.set_oncomplete(None);
            t.set_onabort(None);
            t.set_onerror(None);
            let msg = t
                .error()
                .map(|e| e.message())
                .unwrap_or_else(|| "Storage transaction aborted".into());
            let _ = reject.call1(&JsValue::NULL, &msg.into());
        });
        tx.set_oncomplete(Some(cb.unchecked_ref()));
        tx.set_onabort(Some(err.unchecked_ref()));
    })
}
pub async fn save(messages: &[Message]) -> Result<(), String> {
    for m in messages {
        m.validate()?;
    }
    let db = open().await?;
    let tx = db
        .transaction_with_str_sequence_and_mode(
            &js_sys::Array::of2(&"messages".into(), &"drafts".into()),
            IdbTransactionMode::Readwrite,
        )
        .map_err(error)?;
    let done = transaction_done(&tx);
    let store = tx.object_store("messages").map_err(error)?;
    let writes = async {
        for m in messages {
            let key = JsValue::from_str(&m.key());
            let old = request(&store.get(&key).map_err(error)?).await?;
            if let Some(text) = old.as_string() {
                let old: Message = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if old != *m {
                    return Err(format!("conflicting record {}", m.id));
                }
            } else {
                store
                    .add_with_key(
                        &serde_json::to_string(m).map_err(|e| e.to_string())?.into(),
                        &key,
                    )
                    .map_err(error)?;
            }
            tx.object_store("drafts")
                .map_err(error)?
                .delete(&key)
                .map_err(error)?;
        }
        Ok::<_, String>(())
    }
    .await;
    if let Err(e) = writes {
        let _ = tx.abort();
        let _ = JsFuture::from(done).await;
        db.close();
        return Err(e);
    }
    let result = JsFuture::from(done).await.map_err(error).map(|_| ());
    db.close();
    result
}
pub async fn checkpoint(m: &Message) -> Result<(), String> {
    m.validate()?;
    let db = open().await?;
    let tx = db
        .transaction_with_str_sequence_and_mode(
            &js_sys::Array::of2(&"messages".into(), &"drafts".into()),
            IdbTransactionMode::Readwrite,
        )
        .map_err(error)?;
    let done = transaction_done(&tx);
    let store = tx.object_store("drafts").map_err(error)?;
    let complete = request(
        &tx.object_store("messages")
            .map_err(error)?
            .get(&m.key().into())
            .map_err(error)?,
    )
    .await?;
    let old = request(&store.get(&m.key().into()).map_err(error)?)
        .await?
        .as_string()
        .and_then(|s| serde_json::from_str::<Message>(&s).ok());
    if complete.is_undefined()
        && old
            .as_ref()
            .is_none_or(|old| old.text.len() <= m.text.len())
    {
        store
            .put_with_key(
                &serde_json::to_string(m).map_err(|e| e.to_string())?.into(),
                &m.key().into(),
            )
            .map_err(error)?;
    }
    let result = JsFuture::from(done).await.map_err(error).map(|_| ());
    db.close();
    result
}
pub async fn load() -> Result<RoomArchiveV1, String> {
    let db = open().await?;
    let tx = db.transaction_with_str("messages").map_err(error)?;
    let values = request(
        &tx.object_store("messages")
            .map_err(error)?
            .get_all()
            .map_err(error)?,
    )
    .await?;
    let mut archive = RoomArchiveV1::default();
    for v in js_sys::Array::from(&values).iter() {
        let text = v.as_string().ok_or("invalid stored record")?;
        archive
            .messages
            .push(serde_json::from_str(&text).map_err(|e| format!("stored record: {e}"))?);
    }
    db.close();
    archive.validate()?;
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        if let Ok(Some(text)) = s.get_item("nell.journal.v1") {
            let legacy = lucy_room_memory::legacy_messages(&text, &room_url(), &device())?;
            let incoming = RoomArchiveV1 {
                messages: legacy,
                ..Default::default()
            };
            if archive.merge(&incoming)? > 0 {
                save(&incoming.messages).await?;
            }
        }
    }
    // Visible checkpoints survive a page closing during a generation.
    if let Some(text) = cache_get("visible-draft").await? {
        let mut m: Message = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        m.provenance.status = "interrupted".into();
        if !m.text.is_empty() && !archive.messages.iter().any(|old| old.key() == m.key()) {
            save(&[m.clone()]).await?;
            archive.messages.push(m);
        }
        cache_remove("visible-draft").await?;
    }
    let db = open().await?;
    let tx = db.transaction_with_str("drafts").map_err(error)?;
    let drafts = request(
        &tx.object_store("drafts")
            .map_err(error)?
            .get_all()
            .map_err(error)?,
    )
    .await?;
    db.close();
    for v in js_sys::Array::from(&drafts).iter() {
        let mut m: Message = serde_json::from_str(&v.as_string().ok_or("invalid draft")?)
            .map_err(|e| e.to_string())?;
        m.provenance.status = "interrupted".into();
        if !m.text.is_empty() && !archive.messages.iter().any(|old| old.key() == m.key()) {
            save(&[m.clone()]).await?;
            archive.messages.push(m);
        }
    }
    archive
        .messages
        .sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)));
    Ok(archive)
}
pub async fn cache_get(key: &str) -> Result<Option<String>, String> {
    let db = open().await?;
    let tx = db.transaction_with_str("cache").map_err(error)?;
    let r = request(
        &tx.object_store("cache")
            .map_err(error)?
            .get(&key.into())
            .map_err(error)?,
    )
    .await;
    db.close();
    Ok(r?.as_string())
}
pub async fn cache_put(key: &str, text: &str) -> Result<(), String> {
    let db = open().await?;
    let tx = db
        .transaction_with_str_and_mode("cache", IdbTransactionMode::Readwrite)
        .map_err(error)?;
    let done = transaction_done(&tx);
    tx.object_store("cache")
        .map_err(error)?
        .put_with_key(&text.into(), &key.into())
        .map_err(error)?;
    let result = JsFuture::from(done).await.map_err(error).map(|_| ());
    db.close();
    result
}
pub async fn cache_remove(key: &str) -> Result<(), String> {
    let db = open().await?;
    let tx = db
        .transaction_with_str_and_mode("cache", IdbTransactionMode::Readwrite)
        .map_err(error)?;
    let done = transaction_done(&tx);
    tx.object_store("cache")
        .map_err(error)?
        .delete(&key.into())
        .map_err(error)?;
    let result = JsFuture::from(done).await.map_err(error).map(|_| ());
    db.close();
    result
}
pub async fn forget() -> Result<(), String> {
    let db = open().await?;
    for name in ["messages", "cache", "drafts"] {
        let tx = db
            .transaction_with_str_and_mode(name, IdbTransactionMode::Readwrite)
            .map_err(error)?;
        let done = transaction_done(&tx);
        tx.object_store(name)
            .map_err(error)?
            .clear()
            .map_err(error)?;
        JsFuture::from(done).await.map_err(error)?;
    }
    db.close();
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        s.remove_item("nell.journal.v1").map_err(error)?;
    }
    Ok(())
}
pub fn download(archive: &RoomArchiveV1) -> Result<(), String> {
    let mut archive = archive.clone();
    archive.exported_at = timestamp();
    let parts = js_sys::Array::new();
    parts.push(&archive.json()?.into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(error)?;
    let doc = web_sys::window().unwrap().document().unwrap();
    let a: web_sys::HtmlAnchorElement = doc
        .create_element("a")
        .map_err(error)?
        .dyn_into()
        .map_err(|_| "not an anchor".to_string())?;
    a.set_href(&url);
    a.set_download("lucy-room-memory.json");
    doc.body()
        .ok_or("no body")?
        .append_child(&a)
        .map_err(error)?;
    a.click();
    a.remove();
    let cleanup = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    web_sys::window()
        .unwrap()
        .set_timeout_with_callback_and_timeout_and_arguments_0(cleanup.unchecked_ref(), 30_000)
        .map_err(error)?;
    Ok(())
}
pub async fn yield_now() {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
        } else {
            let scope: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
            let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
        }
    });
    let _ = JsFuture::from(p).await;
}
