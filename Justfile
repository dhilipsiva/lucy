ci:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --workspace
    python3 -m unittest discover -s scripts -p 'test_self_model_study.py'

ci-desktop:
    cargo clippy --all-targets --features desktop-ui -- -D warnings
    cargo test --features desktop-ui

build:
    cargo build --release --locked

verify-pins:
    python3 scripts/verify-pins.py
