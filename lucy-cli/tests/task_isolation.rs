#![cfg(unix)]
use lucy_cli::{
    store::Store,
    task_model::{self, Budget, Check, Criterion, Definition, Observer},
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn run(root: &Path, workspace: &Path, input: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lucy"))
        .args(["self-model", "task", "run", "--stdin", "--root"])
        .arg(root)
        .arg("--workspace")
        .arg(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn definition() -> Definition {
    Definition {
        source: "fixture".into(),
        session: "fixture-session".into(),
        title: "Check execution receipt failure".into(),
        family: "verification".into(),
        method: "fixture".into(),
        model: "fixture".into(),
        environment: "fixture".into(),
        criteria: vec![Criterion {
            id: "marker".into(),
            description: "Marker exists".into(),
            check: Check::FileExists {
                path: "action-marker".into(),
            },
        }],
        budget: Budget::default(),
    }
}

#[test]
fn recording_failure_preserves_the_execution_result_without_repeating_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let moved = temp.path().join("preserved");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let store = Store::open(&root).unwrap();
    let task = task_model::begin(&store, &workspace, definition(), Observer::Cli).unwrap();
    drop(store);
    let result = run(
        &root,
        &workspace,
        json!({"task":task.id,"argv":["sh","-c",
        "printf done > action-marker; mv \"$1\" \"$2\"; printf blocked > \"$1\"","fixture",root,moved]}),
    );
    assert_eq!(result["execution"]["exit_code"], 0);
    assert!(result["recording_error"].is_string());
    assert!(result["receipt"].is_null());
    assert_eq!(
        std::fs::read_to_string(workspace.join("action-marker")).unwrap(),
        "done"
    );
    let saved = Store::open(&moved).unwrap();
    let history = task_model::History::read(&saved).unwrap();
    assert_eq!(history.usage(&task.id).pending_actions, 1);
    assert_eq!(history.usage(&task.id).steps, 1);
}

#[test]
fn study_refuses_real_overlapping_or_unmarked_state_before_inference() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("study");
    let real = temp.path().join("real");
    let home = base.join("identity");
    let data = base.join("data");
    let workspace = base.join("workspace");
    for path in [&real, &home, &data, &workspace] {
        std::fs::create_dir_all(path).unwrap();
    }
    std::fs::write(real.join("sentinel"), "unchanged").unwrap();
    for fault in ["unmarked", "real", "overlap"] {
        if fault != "unmarked" {
            std::fs::write(base.join(".lucy-isolated-study"), "fixture").unwrap();
        }
        let root = if fault == "real" { &real } else { &data };
        let identity = if fault == "overlap" { &data } else { &home };
        let output = Command::new(env!("CARGO_BIN_EXE_lucy"))
            .args(["self-model", "task", "study-trial", "--stdin", "--root"])
            .arg(root)
            .arg("--workspace")
            .arg(&workspace)
            .env("LUCY_HOME", identity)
            .env("LUCY_DATA_DIR", root)
            .env("LUCY_STUDY_ROOT", &base)
            .env("LUCY_STUDY_REAL_DATA", &real)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("study refuses"),
            "{fault}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        std::fs::read_to_string(real.join("sentinel")).unwrap(),
        "unchanged"
    );
    assert_eq!(std::fs::read_dir(&data).unwrap().count(), 0);
}
