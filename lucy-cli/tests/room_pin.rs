//! The Rust room compiles the same pinned engine core as the CLI.
#[test]
fn room_and_cli_share_the_full_nibli_revision() {
    let cargo = include_str!("../../Cargo.toml");
    let room = include_str!("../../room/Cargo.toml");
    let revision = cargo
        .lines()
        .find(|l| l.trim_start().starts_with("nibli-engine"))
        .and_then(|l| l.split("rev = \"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("CLI pins nibli-engine");
    assert_eq!(revision.len(), 40);
    let lines: Vec<_> = room.lines().filter(|l| l.starts_with("nibli-")).collect();
    assert!(!lines.is_empty());
    for line in lines {
        assert!(
            line.contains(&format!("rev = \"{revision}\"")),
            "room engine pin differs: {line}"
        );
    }
}
