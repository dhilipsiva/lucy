//! Writes a small signed-format public peer bundle (manifest and parts) for the conversation's
//! headless sync check (roadmap/N9.md, acceptance 6): `cargo run --example peer_fixture -- DIR`.
use lucy_room_memory::transport;
use serde_json::json;

fn main() {
    let dir = std::env::args().nth(1).expect("an output directory");
    let mut snapshot = String::new();
    for (i, text) in [
        "A record from my test peer: Enma talks all day.",
        "Another record from my test peer, about the room.",
    ]
    .iter()
    .enumerate()
    {
        let payload = json!({"record":{"payload":{"id":format!("test-{i}"),"text":text,"private":false,"channel":"user","kind":"message","timestamp":"2026-11-12T00:00:00Z"}}});
        snapshot.push_str(&format!(
            "record({}, {}, Snapshot, Json).\n",
            json!(format!("target-{i}")),
            json!(payload.to_string())
        ));
    }
    let bundle = transport::bundle(
        json!({"identity":"test","repository":"example/memory","scope":"test","public":true,"ref":"lucy-memory","schema":"lucy-operations-v1"}),
        String::new(),
        snapshot,
        2,
    )
    .expect("bundle");
    for (path, text) in &bundle.files {
        let p = std::path::Path::new(&dir).join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        println!("{path} {}", text.len());
    }
}
