//! How long the engine takes per query on my files, natively and in release: evidence beside
//! the browser's own timings (roadmap/N8-evidence.json). Printed, never asserted.
use lucy_room::mind::{Mind, STANDING_QUERIES};
use std::path::Path;
use std::time::Instant;

#[test]
fn query_times_on_the_files() {
    let read = |n: &str| {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("lucy")
                .join(n),
        )
        .unwrap()
    };
    let mut mind = Mind::new();
    let t0 = Instant::now();
    mind.assert_file(
        "constitution",
        &read("constitution.nibli"),
        "file",
        "the repository",
    );
    mind.assert_file("memory", &read("memory.nibli"), "file", "the repository");
    mind.assert_standing_fact().unwrap();
    eprintln!("load: {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    let now = || Instant::now().duration_since(*START).as_secs_f64() * 1000.0;
    for q in STANDING_QUERIES
        .iter()
        .chain(["protest(Lucy).", "bird(Enma).", "owns(Dhilipsiva, Bucchi)."].iter())
    {
        let a = mind.ask(q, now).unwrap();
        eprintln!("{q} → {} in {:.1} ms", a.status, a.ms);
    }
}
static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
