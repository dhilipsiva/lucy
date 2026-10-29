use super::*;
use crate::store::{Record, Store, digest};
use serde_json::json;
use std::collections::BTreeMap;

fn definition(check: Check) -> Definition {
    Definition {
        source: "test".into(),
        session: "actual-fixture-session".into(),
        title: "Check the requested artifact".into(),
        family: "verification".into(),
        method: "inspect".into(),
        model: "fixture@pinned".into(),
        environment: "fixture-v1".into(),
        criteria: vec![Criterion {
            id: "output".into(),
            description: "The required output is present".into(),
            check,
        }],
        budget: Budget::default(),
    }
}
fn file() -> Check {
    Check::FileExists {
        path: "output.txt".into(),
    }
}
fn outcome(entry: &Entry) -> Verdict {
    match entry.event {
        Event::Finished { verdict, .. } | Event::Checked { verdict, .. } => verdict,
        _ => panic!("not an outcome"),
    }
}

#[test]
fn study_preflight_cannot_retrieve_an_unassigned_calibration_source() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let old = Record {
        origin: "fixture:calibration".into(),
        payload: json!({"id":"old","text":"calibration canary","private":true}),
    };
    store.record(&old).unwrap();
    let task = begin(
        &store,
        temp.path(),
        definition(Check::Record {
            origin: old.origin.clone(),
            record: "old".into(),
            sha256: hash(&old),
        }),
        Observer::Native,
    )
    .unwrap();
    let empty = std::collections::BTreeSet::new();
    let hidden = inspect_inputs_scoped(
        &store,
        temp.path(),
        &task,
        LessonKind::RetrieveSources,
        Some(&empty),
    )
    .unwrap();
    assert_eq!(hidden["observations"][0]["found"], false);
    assert!(!hidden.to_string().contains("calibration canary"));
    let allowed = std::collections::BTreeSet::from([old.key()]);
    let visible = inspect_inputs_scoped(
        &store,
        temp.path(),
        &task,
        LessonKind::RetrieveSources,
        Some(&allowed),
    )
    .unwrap();
    assert_eq!(visible["observations"][0]["excerpt"], "calibration canary");
}

#[test]
fn study_write_approval_is_bounded_and_independent_of_correctness() {
    let request: study::TrialRequest = serde_json::from_value(json!({
        "case":"synthetic","disguise":"a","arm":"full","prompt":"fixture",
        "definition":definition(file()),"oracle":[],"inputs_digest":digest(b"fixture"),
        "output_writes":[{"path":"output.txt","max_bytes":5}],
        "approvals":[{"tool":"command","arguments":{"argv":["true"]}}]
    }))
    .unwrap();
    for text in ["right", "wrong"] {
        assert!(
            request.allows(&json!({"tool":"write","arguments":{"path":"output.txt","text":text}}))
        );
    }
    assert!(
        !request.allows(&json!({"tool":"write","arguments":{"path":"other.txt","text":"right"}}))
    );
    assert!(
        !request.allows(&json!({"tool":"write","arguments":{"path":"output.txt","text":"longer"}}))
    );
    assert!(!request.allows(&json!({"tool":"command","arguments":{"argv":["false"]}})));
    assert!(request.allows(&json!({"tool":"command","arguments":{"argv":["true"]}})));
}

#[test]
fn execution_success_does_not_establish_task_completion_and_receipts_survive_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let store = Store::open(&root).unwrap();
    let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    assert_eq!(task.probability_ppm, 500_000);
    let action = reserve_action(&store, &task.id, "command", &json!(["true"])).unwrap();
    let receipt =
        complete_action(&store, &task.id, &action, Verdict::Success, 12, Some(0)).unwrap();
    assert_eq!(
        complete_action(&store, &task.id, &action, Verdict::Success, 12, Some(0)).unwrap(),
        receipt
    );
    assert!(complete_action(&store, &task.id, &action, Verdict::Failure, 12, Some(1)).is_err());
    let final_entry = finish(&store, temp.path(), &task.id, false).unwrap();
    assert_eq!(outcome(&final_entry), Verdict::Inconclusive);
    assert_eq!(
        finish(&store, temp.path(), &task.id, false).unwrap(),
        final_entry
    );
    assert!(reserve_action(&store, &task.id, "retry", &json!({})).is_err());
    let originals = store.all().unwrap();
    assert!(originals.iter().all(|r| r.payload["private"] == true));
    drop(store);
    let store = Store::open(&root).unwrap();
    assert_eq!(store.all().unwrap(), originals);
    History::read(&store).unwrap().audit().unwrap();
    // Restore the immutable records into a separate store, without a derived cache.
    let restored = Store::open(&temp.path().join("restored")).unwrap();
    for record in originals {
        restored.record(&record).unwrap();
    }
    assert_eq!(
        History::read(&restored).unwrap().outcome(&task.id),
        Some(&final_entry)
    );
}

#[test]
fn stale_positive_and_negative_file_evidence_is_inconclusive_and_revision_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    assert_eq!(
        outcome(&check(&store, temp.path(), &task.id, "output").unwrap()),
        Verdict::Failure
    );
    std::fs::write(temp.path().join("output.txt"), "first").unwrap();
    assert_eq!(
        outcome(&finish(&store, temp.path(), &task.id, false).unwrap()),
        Verdict::Inconclusive
    );
    let next = revise(
        &store,
        temp.path(),
        &task.id,
        definition(file()),
        Observer::Cli,
    )
    .unwrap();
    assert_eq!(next.revision_of, Some(task.id));
    assert_eq!(
        outcome(&check(&store, temp.path(), &next.id, "output").unwrap()),
        Verdict::Success
    );
    std::fs::write(temp.path().join("output.txt"), "changed").unwrap();
    assert_eq!(
        outcome(&finish(&store, temp.path(), &next.id, false).unwrap()),
        Verdict::Inconclusive
    );
    let recovered = revise(
        &store,
        temp.path(),
        &next.id,
        definition(file()),
        Observer::Cli,
    )
    .unwrap();
    check(&store, temp.path(), &recovered.id, "output").unwrap();
    assert_eq!(
        outcome(&finish(&store, temp.path(), &recovered.id, false).unwrap()),
        Verdict::Success
    );
}

#[test]
fn hashes_json_and_records_require_actual_matching_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    std::fs::write(temp.path().join("output.txt"), "{\"ready\":true}").unwrap();
    let record = Record {
        origin: "fixture:source".into(),
        payload: json!({"id":"source-1","private":true,"text":"an exact source"}),
    };
    store.record(&record).unwrap();
    for (check_kind, expected) in [
        (
            Check::FileSha256 {
                path: "output.txt".into(),
                sha256: digest(b"{\"ready\":true}"),
            },
            Verdict::Success,
        ),
        (
            Check::JsonEquals {
                path: "output.txt".into(),
                pointer: "/ready".into(),
                expected: json!(true),
            },
            Verdict::Success,
        ),
        (
            Check::Record {
                origin: record.origin.clone(),
                record: "source-1".into(),
                sha256: hash(&record),
            },
            Verdict::Success,
        ),
        (
            Check::Record {
                origin: record.origin.clone(),
                record: "missing".into(),
                sha256: hash(&record),
            },
            Verdict::Failure,
        ),
        (
            Check::JsonEquals {
                path: "output.txt".into(),
                pointer: "/ready".into(),
                expected: json!(false),
            },
            Verdict::Failure,
        ),
        (
            Check::Reported {
                statement: "The model said it succeeded".into(),
            },
            Verdict::Inconclusive,
        ),
    ] {
        let task = begin(&store, temp.path(), definition(check_kind), Observer::Cli).unwrap();
        assert_eq!(
            outcome(&check(&store, temp.path(), &task.id, "output").unwrap()),
            expected
        );
        assert_eq!(
            outcome(&finish(&store, temp.path(), &task.id, false).unwrap()),
            expected
        );
    }
}

#[test]
fn interrupted_execution_and_budget_exhaustion_never_verify() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let mut def = definition(file());
    def.budget.steps = 1;
    let task = begin(&store, temp.path(), def, Observer::Cli).unwrap();
    reserve_action(&store, &task.id, "unfinished", &json!({})).unwrap();
    assert!(check(&store, temp.path(), &task.id, "output").is_err());
    assert!(reserve_action(&store, &task.id, "repeat", &json!({})).is_err());
    assert_eq!(
        outcome(&finish(&store, temp.path(), &task.id, false).unwrap()),
        Verdict::Inconclusive
    );
    let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    overhead(&store, &task.id, "inference", Budget::default().active_ms).unwrap();
    assert!(check(&store, temp.path(), &task.id, "output").is_err());
    assert_eq!(
        outcome(&finish(&store, temp.path(), &task.id, false).unwrap()),
        Verdict::Inconclusive
    );
}

#[test]
fn lesson_candidates_preserve_failure_and_contrary_evidence_but_do_not_enable_themselves() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    for _ in 0..2 {
        let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
        check(&store, temp.path(), &task.id, "output").unwrap();
        finish(&store, temp.path(), &task.id, false).unwrap();
    }
    let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    assert_eq!(task.probability_ppm, 250_000);
    assert_eq!(task.basis.len(), 2);
    assert!(
        task.lessons
            .iter()
            .any(|l| l.kind == LessonKind::InspectInputs && l.active)
    );
    assert!(!study::enabled(&store, &task.scope).unwrap());
    finish(&store, temp.path(), &task.id, true).unwrap();
    std::fs::write(temp.path().join("output.txt"), "ready").unwrap();
    for _ in 0..2 {
        let task = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
        check(&store, temp.path(), &task.id, "output").unwrap();
        finish(&store, temp.path(), &task.id, false).unwrap();
    }
    let history = History::read(&store).unwrap();
    let lessons = history.lessons(&task.scope);
    assert!(lessons.iter().all(|l| !l.active));
    assert_eq!(lessons[0].contrary.len(), 2);
    for field in ["model", "environment", "source"] {
        let mut def = definition(file());
        match field {
            "model" => def.model = "other".into(),
            "source" => def.source = "other".into(),
            _ => def.environment = "other".into(),
        };
        let cold = begin(&store, temp.path(), def, Observer::Cli).unwrap();
        assert_eq!(cold.probability_ppm, 500_000);
        assert!(cold.lessons.is_empty());
    }
    let native = begin(&store, temp.path(), definition(file()), Observer::Native).unwrap();
    assert!(native.basis.is_empty());
}

#[test]
fn graph_faults_are_rejected_by_rust_and_nibli_and_controls_remain_valid() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let first = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    check(&store, temp.path(), &first.id, "output").unwrap();
    finish(&store, temp.path(), &first.id, false).unwrap();
    let second = begin(&store, temp.path(), definition(file()), Observer::Cli).unwrap();
    check(&store, temp.path(), &second.id, "output").unwrap();
    finish(&store, temp.path(), &second.id, false).unwrap();
    let history = History::read(&store).unwrap();
    history.audit().unwrap();
    for fault in [
        "missing",
        "scope",
        "future",
        "cycle",
        "criterion",
        "cross_task",
        "version",
        "forged_verdict",
    ] {
        let mut bad = history.clone();
        if fault == "cross_task" || fault == "forged_verdict" {
            let entry = bad
                .entries
                .values_mut()
                .find(|e| e.task == first.id && matches!(e.event, Event::Finished { .. }))
                .unwrap();
            if let Event::Finished {
                checks, verdict, ..
            } = &mut entry.event
            {
                if fault == "cross_task" {
                    checks.push(history.latest_checks(&second.id)["output"].id.clone());
                } else {
                    *verdict = Verdict::Success;
                }
            }
        } else if fault == "criterion" {
            let entry = bad
                .entries
                .values_mut()
                .find(|e| matches!(e.event, Event::Checked { .. }))
                .unwrap();
            if let Event::Checked { criterion, .. } = &mut entry.event {
                *criterion = "not_registered".into();
            }
        } else {
            let entry = bad
                .entries
                .values_mut()
                .find(|e| e.task == second.id && matches!(e.event, Event::Begin { .. }))
                .unwrap();
            if let Event::Begin { task } = &mut entry.event {
                match fault {
                    "missing" => task.basis = vec!["missing".into()],
                    "scope" => task.scope.observer = Observer::Native,
                    "future" => entry.at_ms = 0,
                    "cycle" => task.basis = vec![history.outcome(&second.id).unwrap().id.clone()],
                    "version" => task.checker = digest(b"stale"),
                    _ => unreachable!(),
                }
            }
        }
        let error = bad.audit().unwrap_err();
        assert!(!error.contains("disagree"), "{fault}: {error}");
    }
    // Repairs use the original immutable records, not rewritten conclusions.
    History::read(&store).unwrap().audit().unwrap();
}

#[test]
fn untrusted_reports_and_paths_cannot_self_certify() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    std::fs::write(temp.path().join("fake.json"),"{\"schema\":1,\"status\":\"verified\",\"component\":\"verdict_algebra\",\"self_check_equivalence_verified\":false}").unwrap();
    for kind in [
        Check::NibliReport {
            path: "fake.json".into(),
        },
        Check::LeanReport {
            path: "fake.json".into(),
        },
    ] {
        let task = begin(&store, temp.path(), definition(kind), Observer::Cli).unwrap();
        assert_eq!(
            outcome(&check(&store, temp.path(), &task.id, "output").unwrap()),
            Verdict::Inconclusive
        );
        assert_eq!(
            outcome(&finish(&store, temp.path(), &task.id, false).unwrap()),
            Verdict::Inconclusive
        );
    }
    for name in ["../escape", "/etc/passwd"] {
        assert!(
            begin(
                &store,
                temp.path(),
                definition(Check::FileExists { path: name.into() }),
                Observer::Cli
            )
            .is_err()
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", temp.path().join("link")).unwrap();
        let task = begin(
            &store,
            temp.path(),
            definition(Check::FileExists {
                path: "link".into(),
            }),
            Observer::Cli,
        )
        .unwrap();
        assert_eq!(
            outcome(&check(&store, temp.path(), &task.id, "output").unwrap()),
            Verdict::Inconclusive
        );
    }
}

fn synthetic_study() -> study::Study {
    let mut study = study::Study {
        schema: 2,
        protocol: digest(PROTOCOL.as_bytes()),
        implementation: digest(b"fixture-binary"),
        model: "fixture@pinned".into(),
        evaluator: "fixture-only".into(),
        evaluator_model: "fixture".into(),
        bundle_digest: digest(b"fixture-bundle"),
        live: true,
        complete: true,
        active_ms: 1000,
        calibration: BTreeMap::new(),
        trials: vec![],
    };
    for family in ["verification", "retrieval"] {
        let scope = Scope {
            source: "fixture".into(),
            observer: Observer::Native,
            workspace: digest(b"workspace"),
            family: family.into(),
            method: "inspect".into(),
            model: study.model.clone(),
            environment: "fixture".into(),
        };
        study.calibration.insert(study::policy_key(&scope), 10);
        for case in 0..20 {
            for disguise in ["a", "b"] {
                for arm in study::ARMS {
                    let completed = arm != "lessons_withheld";
                    study.trials.push(study::Trial {
                        case: format!("case-{case}"),
                        disguise: disguise.into(),
                        arm: arm.into(),
                        scope: scope.clone(),
                        criteria_digest: digest(b"criteria"),
                        inputs_digest: digest(b"input"),
                        budget: Budget::default(),
                        completed,
                        false_verified: false,
                        budget_violation: false,
                        inconclusive: false,
                        probability_ppm: 500_000,
                        observed: Some(completed),
                        active_ms: 1,
                        checking_ms: 0,
                        task: format!("{family}-{case}-{disguise}-{arm}"),
                        journal_digest: digest(b"journal"),
                        oracle_digest: digest(b"oracle"),
                        implementation: study.implementation.clone(),
                        model: study.model.clone(),
                    });
                }
            }
        }
    }
    study
}

#[test]
fn study_gate_requires_complete_paired_safe_trials_and_does_not_claim_empirical_evidence() {
    // Synthetic data tests gate mathematics only; it is not an empirical study.
    let study = synthetic_study();
    let scored = study::score(&study).unwrap();
    assert!(
        scored["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["eligible"] == true)
    );
    assert_eq!(scored["s6a_complete"], false);
    for fault in [
        "incomplete",
        "not_live",
        "budget",
        "false_success",
        "time",
        "null",
        "partial",
        "criteria",
    ] {
        let mut bad = study.clone();
        match fault {
            "incomplete" => bad.complete = false,
            "not_live" => bad.live = false,
            "time" => bad.active_ms = 3_600_001,
            "partial" => {
                bad.trials.pop();
            }
            "budget" => bad.trials[0].budget_violation = true,
            "false_success" => bad.trials[0].false_verified = true,
            "criteria" => bad.trials[0].criteria_digest = digest(b"weakened"),
            "null" => {
                for t in &mut bad.trials {
                    t.completed = false;
                    t.observed = Some(false);
                }
            }
            _ => unreachable!(),
        }
        let report = study::score(&bad).unwrap();
        assert!(
            report["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["eligible"] == false),
            "{fault}"
        );
    }
    let mut bad = study.clone();
    bad.trials.push(bad.trials[0].clone());
    assert!(study::score(&bad).is_err());
    let mut bad = study;
    bad.trials[0].model = "changed".into();
    assert!(study::score(&bad).is_err());
}

#[test]
fn cli_cannot_forge_native_results_and_activation_requires_retrievable_study() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let store = Store::open(&root).unwrap();
    let task = begin(&store, temp.path(), definition(file()), Observer::Native).unwrap();
    let scope = study::policy_key(&task.scope);
    assert!(
        study::policy_command(
            &store,
            &json!({"scope":scope,"enabled":true,"study":"absent"}).to_string()
        )
        .is_err()
    );
    study::policy_command(
        &store,
        &json!({"scope":scope,"enabled":false,"study":null}).to_string(),
    )
    .unwrap();
    assert!(!study::enabled(&store, &task.scope).unwrap());
    let records = store.all().unwrap().len();
    drop(store);
    let args = vec![
        "finish".into(),
        "--root".into(),
        root.to_str().unwrap().into(),
        "--workspace".into(),
        temp.path().to_str().unwrap().into(),
        "--stdin".into(),
    ];
    assert!(
        command(&args, &json!({"task":task.id}).to_string())
            .unwrap_err()
            .contains("native")
    );
    assert_eq!(Store::open(&root).unwrap().all().unwrap().len(), records);
}

#[test]
fn policy_is_revoked_by_missing_study_scope_change_or_explicit_disable() {
    // Isolated synthetic gate fixture; not a scored independent study.
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let mut data = synthetic_study();
    data.implementation = study::implementation().unwrap();
    for trial in &mut data.trials {
        trial.implementation = data.implementation.clone();
    }
    let report = study::score(&data).unwrap();
    let id = hash(&report);
    let scope = data.trials[0].scope.clone();
    let key = study::policy_key(&scope);
    let original = Record {
        origin: "lucy:task-study:v1".into(),
        payload: json!({"id":id,"private":true,"text":"Synthetic mechanism fixture","report":report}),
    };
    store.record(&original).unwrap();
    study::policy_command(
        &store,
        &json!({"scope":key,"enabled":true,"study":id}).to_string(),
    )
    .unwrap();
    assert!(study::enabled(&store, &scope).unwrap());
    let mut changed = scope.clone();
    changed.workspace = digest(b"different workspace");
    assert!(!study::enabled(&store, &changed).unwrap());
    changed = scope.clone();
    changed.model = "changed revision".into();
    assert!(!study::enabled(&store, &changed).unwrap());
    let missing = Store::open(&temp.path().join("missing-study")).unwrap();
    for record in store
        .all()
        .unwrap()
        .into_iter()
        .filter(|r| r.origin != "lucy:task-study:v1")
    {
        missing.record(&record).unwrap();
    }
    assert!(!study::enabled(&missing, &scope).unwrap());
    study::policy_command(
        &store,
        &json!({"scope":key,"enabled":false,"study":null}).to_string(),
    )
    .unwrap();
    assert!(!study::enabled(&store, &scope).unwrap());
    assert!(store.all().unwrap().contains(&original));
}

#[test]
fn native_study_scoring_reopens_serialized_receipts_and_rejects_tampering() {
    // A synthetic mechanism fixture, never an empirical model trial.
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let store = Store::open(&temp.path().join("memory")).unwrap();
    let criteria = definition(Check::JsonEquals {
        path: "answer.json".into(),
        pointer: "/answer".into(),
        expected: json!(42),
    });
    let task = begin(&store, &workspace, criteria.clone(), Observer::Native).unwrap();
    let answer = b"{\"answer\":42}";
    std::fs::write(workspace.join("answer.json"), answer).unwrap();
    check(&store, &workspace, &task.id, "output").unwrap();
    assert_eq!(
        outcome(&finish(&store, &workspace, &task.id, false).unwrap()),
        Verdict::Success
    );
    let history = History::read(&store).unwrap();
    let usage = history.usage(&task.id);
    let mut oracle_definition = criteria.clone();
    oracle_definition.source = "synthetic-independent-oracle".into();
    oracle_definition.session = "synthetic-oracle-session".into();
    let oracle_task = begin(&store, &workspace, oracle_definition, Observer::Cli).unwrap();
    check(&store, &workspace, &oracle_task.id, "output").unwrap();
    let oracle = finish(&store, &workspace, &oracle_task.id, false).unwrap();
    assert_eq!(outcome(&oracle), Verdict::Success);
    let all = History::read(&store).unwrap();
    let archive = temp.path().join("archive");
    std::fs::create_dir_all(archive.join("inputs")).unwrap();
    std::fs::create_dir(archive.join("outputs")).unwrap();
    std::fs::write(archive.join("outputs/answer.json"), answer).unwrap();
    let checking_ms = history
        .rows(&task.id)
        .iter()
        .map(|entry| match entry.event {
            Event::Checked { active_ms, .. } | Event::Finished { active_ms, .. } => active_ms,
            _ => 0,
        })
        .sum();
    let trial = study::Trial {
        case: "synthetic-calibration".into(),
        disguise: "a".into(),
        arm: "calibration".into(),
        scope: task.scope.clone(),
        criteria_digest: hash(&criteria.criteria),
        inputs_digest: digest(b"synthetic-inputs"),
        budget: task.definition.budget,
        completed: true,
        false_verified: false,
        budget_violation: false,
        inconclusive: false,
        probability_ppm: task.probability_ppm,
        observed: Some(true),
        active_ms: usage.active_ms + 1,
        checking_ms,
        task: task.id.clone(),
        journal_digest: hash(&history.entries),
        oracle_digest: hash(&oracle),
        implementation: digest(b"synthetic-implementation"),
        model: task.scope.model.clone(),
    };
    let original = json!({"schema":2,"trial":trial,"native_active_ms":usage.active_ms,
        "journal":history.entries,"oracle":oracle,"oracle_journal":all.rows(&oracle_task.id),
        "artifacts":{"directory":archive,"inputs":{},"outputs":{"answer.json":digest(answer)},
            "records":[],"oracle":criteria.criteria}});
    let original_path = temp.path().join("original.json");
    std::fs::write(&original_path, serde_json::to_vec(&original).unwrap()).unwrap();
    let study = study::Study {
        schema: 2,
        protocol: digest(PROTOCOL.as_bytes()),
        implementation: trial.implementation.clone(),
        model: trial.model.clone(),
        evaluator: "synthetic-evaluator".into(),
        evaluator_model: "synthetic-evaluator-model".into(),
        bundle_digest: digest(b"synthetic-bundle"),
        live: false,
        complete: false,
        active_ms: trial.active_ms + 1000,
        calibration: BTreeMap::from([(study::policy_key(&trial.scope), 1)]),
        trials: vec![],
    };
    let request =
        json!({"study":study,"originals":[],"calibration_originals":[original_path]}).to_string();
    let report_root = temp.path().join("report");
    let report = study::score_command(&report_root, &request).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["s6a_complete"], false);
    assert_eq!(report["calibration_safe"], true);
    let mut tampered = original.clone();
    tampered["oracle"]["event"]["reason"] = json!("tampered receipt");
    std::fs::write(&original_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(study::score_command(&report_root, &request).is_err());
    std::fs::write(&original_path, serde_json::to_vec(&original).unwrap()).unwrap();
    std::fs::write(archive.join("outputs/answer.json"), b"{\"answer\":43}").unwrap();
    assert!(study::score_command(&report_root, &request).is_err());
}
