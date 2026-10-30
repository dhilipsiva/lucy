use super::*;
use serde_json::Value;

const SPEC: &str = crate::task_model::SPEC;
fn isolated(enabled: bool) -> Context {
    Context {
        enabled,
        cache: Arc::new(Mutex::new(VecDeque::new())),
    }
}
fn run(context: &Context, facts: &str) -> Result<Checked> {
    check_with(
        context.clone(),
        Domain::Task,
        SPEC,
        facts,
        Settings::default(),
    )
}
pub(crate) fn measured(f: impl FnOnce()) -> Value {
    let dir = tempfile::tempdir().unwrap();
    profile::capture(dir.path(), "profile.json", || {
        f();
        Ok(())
    })
    .unwrap();
    serde_json::from_slice(&std::fs::read(dir.path().join("profile.json")).unwrap()).unwrap()
}

#[test]
fn reuse_requires_caller_success_and_bypass_never_uses_or_populates_cache() {
    let ctx = isolated(true);
    let report = measured(|| {
        // A caller-side failure drops the token, leaving the next call cold.
        drop(run(&ctx, "").unwrap());
        run(&ctx, "").unwrap().remember();
        assert!(!run(&ctx, "").unwrap().invalid);
        let bypass = Context {
            enabled: false,
            cache: ctx.cache.clone(),
        };
        run(&bypass, "").unwrap().remember();
        run(&bypass, "# other").unwrap().remember();
    });
    assert_eq!(report["counts"]["nibli_evaluations"], 4);
    assert_eq!(report["counts"]["audit_cache_hits"], 1);
    assert_eq!(ctx.cache.lock().unwrap().len(), 1);
}

#[test]
fn key_binds_every_logical_input_and_engine_setting() {
    let settings = Settings::default();
    let key = fingerprint(Domain::Task, SPEC, "", QUERY, settings);
    for changed in [
        fingerprint(Domain::Legacy, SPEC, "", QUERY, settings),
        fingerprint(Domain::Grouped, SPEC, "", QUERY, settings),
        fingerprint(
            Domain::Task,
            &format!("{SPEC}\n# changed"),
            "",
            QUERY,
            settings,
        ),
        fingerprint(Domain::Task, SPEC, "# changed", QUERY, settings),
        fingerprint(Domain::Task, SPEC, "", "sl_invalid(Other).", settings),
        fingerprint(
            Domain::Task,
            SPEC,
            "",
            QUERY,
            Settings {
                depth: 9,
                ..settings
            },
        ),
        fingerprint(
            Domain::Task,
            SPEC,
            "",
            QUERY,
            Settings {
                materialization: false,
                ..settings
            },
        ),
        fingerprint(
            Domain::Task,
            SPEC,
            "",
            QUERY,
            Settings {
                strict: true,
                ..settings
            },
        ),
        fingerprint(
            Domain::Task,
            SPEC,
            "",
            QUERY,
            Settings {
                existential_import: true,
                ..settings
            },
        ),
    ] {
        assert_ne!(key, changed);
    }
    // Length framing prevents ambiguity between spec and fact boundaries.
    assert_ne!(
        fingerprint(Domain::Task, "ab", "c", QUERY, settings),
        fingerprint(Domain::Task, "a", "bc", QUERY, settings)
    );
}

#[test]
fn invalid_malformed_and_incomplete_results_never_populate() {
    let ctx = isolated(true);
    for _ in 0..2 {
        let invalid = run(&ctx, "exist(Event, TaskEvent, Missing).").unwrap();
        assert!(invalid.invalid);
        invalid.remember();
        assert!(run(&ctx, "this is not an assertion !").is_err());
        let incomplete = check_with(
            ctx.clone(),
            Domain::Task,
            SPEC,
            "",
            Settings {
                materialization: false,
                ..Settings::default()
            },
        );
        assert!(incomplete.err().unwrap().contains("incomplete"));
        assert!(ctx.cache.lock().unwrap().is_empty());
    }
    run(&ctx, "").unwrap().remember();
    assert_eq!(ctx.cache.lock().unwrap().len(), 1);
}

#[test]
fn bounded_eviction_contention_poisoning_and_concurrent_checks_fall_back() {
    let ctx = isolated(true);
    let report = measured(|| {
        for i in 0..=CAPACITY {
            run(&ctx, &format!("# {i}")).unwrap().remember();
        }
        assert_eq!(ctx.cache.lock().unwrap().len(), CAPACITY);
        run(&ctx, "# 0").unwrap().remember(); // evicted entry is evaluated again
        let held = ctx.cache.lock().unwrap();
        run(&ctx, "# 0").unwrap().remember(); // no deadlock or skipped check
        drop(held);
    });
    assert_eq!(report["counts"]["nibli_evaluations"], 67);
    assert_eq!(report["counts"]["audit_cache_evictions"], 2);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                for _ in 0..3 {
                    run(&ctx, "# concurrent").unwrap().remember();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(ctx.cache.lock().unwrap().len(), CAPACITY);
    let cache = ctx.cache.clone();
    assert!(
        std::thread::spawn(move || {
            let _lock = cache.lock().unwrap();
            panic!("intentional cache poisoning");
        })
        .join()
        .is_err()
    );
    let report = measured(|| run(&ctx, "# concurrent").unwrap().remember());
    assert_eq!(report["counts"]["nibli_evaluations"], 1);
}

#[test]
fn resource_exceeded_and_unknown_results_cannot_produce_a_success_token() {
    use nibli_types::logic::{ResourceKind, UnknownReason};
    let ctx = isolated(true);
    let key = fingerprint(Domain::Task, SPEC, "", QUERY, Settings::default());
    let refused = vec![("sl_invalid".into(), "the materialisation budget".into())];
    // Inject typed engine outcomes at the result boundary, without exhausting
    // the dependency's two-million-tuple limit in the ordinary test suite.
    for result in [
        QueryResult::ResourceExceeded(ResourceKind::Depth),
        QueryResult::ResourceExceeded(ResourceKind::Fuel),
        QueryResult::ResourceExceeded(ResourceKind::Memory),
        QueryResult::Unknown(UnknownReason::IncompleteKnowledge),
    ] {
        for complete in [vec![], vec!["sl_invalid".into()]] {
            assert!(
                checked_result(
                    Domain::Task,
                    result.clone(),
                    &complete,
                    &refused,
                    Some((ctx.cache.clone(), key))
                )
                .is_err()
            );
        }
    }
    assert!(
        checked_result(
            Domain::Task,
            QueryResult::False,
            &[],
            &refused,
            Some((ctx.cache.clone(), key))
        )
        .is_err()
    );
    assert!(ctx.cache.lock().unwrap().is_empty());
}

#[test]
fn process_restart_is_cold() {
    for enabled in ["0", "1", "0", "1"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "audit_cache::tests::process_cache_probe",
                "--nocapture",
            ])
            .env("LUCY_AUDIT_CACHE", enabled)
            .env("LUCY_AUDIT_CACHE_PROBE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn process_cache_probe() {
    if std::env::var("LUCY_AUDIT_CACHE_PROBE").as_deref() != Ok("1") {
        return;
    }
    let report = measured(|| {
        check(Domain::Task, SPEC, "").unwrap().remember();
        check(Domain::Task, SPEC, "").unwrap().remember();
    });
    if std::env::var("LUCY_AUDIT_CACHE").as_deref() == Ok("0") {
        assert_eq!(report["counts"]["nibli_evaluations"], 2);
        assert!(report["counts"].get("audit_cache_hits").is_none());
    } else {
        assert_eq!(report["counts"]["nibli_evaluations"], 1);
        assert_eq!(report["counts"]["audit_cache_hits"], 1);
    }
}
