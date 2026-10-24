fn main() {
    let lock = std::fs::read_to_string("../Cargo.lock").expect("workspace Cargo.lock");
    let package = lock
        .split("[[package]]")
        .find(|p| p.lines().any(|line| line == "name = \"nibli-engine\""))
        .expect("pinned Nibli engine");
    let source = package
        .lines()
        .find_map(|l| l.strip_prefix("source = \""))
        .expect("engine Git source");
    let rev = source
        .trim_end_matches('"')
        .rsplit('#')
        .next()
        .expect("Git revision");
    println!("cargo:rustc-env=LUCY_NIBLI_REV={rev}");
    println!("cargo:rerun-if-changed=../Cargo.lock");
}
