use super::*;

fn group() -> Group {
    Group {
        source: "codex".into(),
        measurement: Measurement::Reported,
        capability: "read".into(),
        conditions: Conditions {
            workspace: digest(b"workspace"),
            method: "file-read-v1".into(),
            model_identity: ModelIdentity::Known {
                name: "fixture".into(),
                revision: Some("weights-1".into()),
            },
            implementation: "executor-1".into(),
            environment: digest(b"fixture-environment"),
        },
    }
}
fn forecast(store: &Store) -> Prediction {
    predict(
        store,
        group(),
        "session",
        "The fixture succeeds.",
        BTreeMap::new(),
    )
    .unwrap()
}
fn reviewer() -> Reviewer {
    Reviewer {
        source: "test".into(),
        session: "review-session".into(),
    }
}
fn proposal(store: &Store, target: Group) -> Proposal {
    let mut scope = target.scope();
    scope.context = digest(b"dated legacy context");
    let p = super::super::predict(store, scope, "old-session", "The fixture succeeds.").unwrap();
    let o =
        super::super::observe(store, &p.id, ResultKind::Success, "The fixture completed.").unwrap();
    let support = Record {
        origin: "fixture-support".into(),
        payload: json!({"id":"conditions","private":true,"text":"Fixture evidence for all five comparability conditions."}),
    };
    store.record(&support).unwrap();
    Proposal {
        target,
        candidates: vec![Candidate {
            observation: o.id.clone(),
            prediction_digest: hash(&p),
            observation_digest: hash(&o),
            rationale: [
                "workspace",
                "method",
                "model_identity",
                "implementation",
                "environment",
            ]
            .into_iter()
            .map(|k| {
                (
                    k.into(),
                    "Confirmed by the fixture's immutable supporting record.".into(),
                )
            })
            .collect(),
            support: vec![Evidence::Record {
                origin: support.origin.clone(),
                id: "conditions".into(),
                sha256: hash(&support),
            }],
        }],
    }
}

#[test]
fn dates_metadata_sessions_and_restarts_share_only_matching_conditions() {
    let root = tempfile::tempdir().unwrap();
    {
        let store = Store::open(root.path()).unwrap();
        let p = predict(
            &store,
            group(),
            "yesterday",
            "The fixture succeeds.",
            BTreeMap::from([
                ("date".into(), "yesterday".into()),
                ("generation".into(), "29".into()),
            ]),
        )
        .unwrap();
        observe(&store, &p.id, ResultKind::Success, "Observed.").unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let p = predict(
        &store,
        group(),
        "today",
        "A differently worded expectation.",
        BTreeMap::from([
            ("generation".into(), "30".into()),
            ("date".into(), "today".into()),
        ]),
    )
    .unwrap();
    assert_eq!(p.probability_ppm, 666_666);
    assert_eq!(p.basis.len(), 1);
    let mut variants = Vec::new();
    let mut g = group();
    g.source = "other".into();
    variants.push(g);
    let mut g = group();
    g.measurement = Measurement::Runtime;
    variants.push(g);
    let mut g = group();
    g.capability = "write".into();
    variants.push(g);
    let mut g = group();
    g.conditions.workspace = digest(b"other");
    variants.push(g);
    let mut g = group();
    g.conditions.method = "other".into();
    variants.push(g);
    let mut g = group();
    g.conditions.implementation = "other".into();
    variants.push(g);
    let mut g = group();
    g.conditions.environment = digest(b"other");
    variants.push(g);
    let mut g = group();
    g.conditions.model_identity = ModelIdentity::Known {
        name: "other".into(),
        revision: None,
    };
    variants.push(g);
    let mut g = group();
    g.conditions.model_identity = ModelIdentity::Known {
        name: "fixture".into(),
        revision: Some("weights-2".into()),
    };
    variants.push(g);
    for g in variants {
        assert_eq!(
            predict(&store, g, "session", "Fixture.", BTreeMap::new())
                .unwrap()
                .probability_ppm,
            500_000
        );
    }
    let summary = History::read(&store).unwrap().summary().unwrap();
    assert!(summary.to_string().find("yesterday").is_none());
    assert_eq!(summary["grouping_version"], 2);
}

#[test]
fn review_preview_apply_revoke_preserves_originals_and_old_forecasts() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let proposal = proposal(&store, group());
    let originals = store.all().unwrap();
    let old_summary = LegacyHistory::read(&store).unwrap().summary().unwrap();
    assert_eq!(forecast(&store).probability_ppm, 500_000);
    let before = store.all().unwrap();
    let p = preview(&store, proposal.clone()).unwrap();
    assert_eq!(store.all().unwrap(), before);
    assert_eq!(p.probability_ppm, 666_666);
    let review = apply(&store, p.clone(), &p.digest, reviewer()).unwrap();
    assert_eq!(
        apply(&store, p.clone(), &p.digest, reviewer()).unwrap(),
        review
    );
    // A second approved link must not count the same outcome twice.
    let another = preview(&store, proposal).unwrap();
    let duplicate = apply(&store, another.clone(), &another.digest, reviewer()).unwrap();
    let learned = forecast(&store);
    assert_eq!(learned.probability_ppm, 666_666);
    assert_eq!(learned.basis.len(), 1);
    assert_eq!(learned.review_basis.len(), 1);
    observe(
        &store,
        &learned.id,
        ResultKind::Failure,
        "Observed failure.",
    )
    .unwrap();
    revoke(
        &store,
        &review.id,
        reviewer(),
        "Withdraw this interpretation.",
    )
    .unwrap();
    revoke(
        &store,
        &duplicate.id,
        reviewer(),
        "Withdraw the duplicate too.",
    )
    .unwrap();
    let after = forecast(&store);
    assert_eq!(after.probability_ppm, 333_333);
    assert!(after.review_basis.is_empty());
    let history = History::read(&store).unwrap();
    history.audit().unwrap();
    assert_eq!(history.predictions[&learned.id], learned);
    assert_eq!(history.legacy.summary().unwrap(), old_summary);
    for r in originals {
        assert_eq!(
            store
                .get(&r.origin, r.payload["id"].as_str().unwrap())
                .unwrap(),
            Some(r)
        );
    }
    let summary = history.summary().unwrap();
    assert_eq!(summary["scopes"][0]["scored_forecasts"], 1);
    assert_eq!(summary["scopes"][0]["reviewed_observations"], 0);
}

#[test]
fn stale_previews_and_unsupported_or_conflicting_candidates_cannot_apply() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let original = proposal(&store, group());
    let p = preview(&store, original.clone()).unwrap();
    forecast(&store);
    assert!(
        apply(&store, p.clone(), &p.digest, reviewer())
            .unwrap_err()
            .contains("stale")
    );
    let mut bads = Vec::new();
    let mut bad = original.clone();
    bad.target.source = "other".into();
    bads.push(bad);
    let mut bad = original.clone();
    bad.target.measurement = Measurement::Runtime;
    bads.push(bad);
    let mut bad = original.clone();
    bad.target.capability = "write".into();
    bads.push(bad);
    let mut bad = original.clone();
    bad.candidates[0].rationale.remove("model_identity");
    bads.push(bad);
    let mut bad = original.clone();
    bad.candidates[0].support.clear();
    bads.push(bad);
    let mut bad = original.clone();
    bad.candidates[0].prediction_digest = digest(b"wrong");
    bads.push(bad);
    let mut bad = original;
    bad.candidates[0].support[0] = Evidence::Record {
        origin: "missing".into(),
        id: "record".into(),
        sha256: digest(b"missing"),
    };
    bads.push(bad);
    for bad in bads {
        let p = preview(&store, bad).unwrap();
        assert!(!p.excluded.is_empty());
        assert!(apply(&store, p.clone(), &p.digest, reviewer()).is_err());
    }
    assert!(History::read(&store).unwrap().reviews.is_empty());
}

#[test]
fn mixed_archives_survive_backup_restore_and_legacy_readers_ignore_v2() {
    let root = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    let copy = tempfile::tempdir().unwrap();
    let (manifest, before, legacy) = {
        let store = Store::open(root.path()).unwrap();
        let proposal = proposal(&store, group());
        let p = preview(&store, proposal).unwrap();
        apply(&store, p.clone(), &p.digest, reviewer()).unwrap();
        let p = forecast(&store);
        observe(&store, &p.id, ResultKind::Success, "Observed.").unwrap();
        (
            store.backup(backup.path()).unwrap(),
            History::read(&store).unwrap().summary().unwrap(),
            LegacyHistory::read(&store).unwrap().summary().unwrap(),
        )
    };
    let store = Store::open(copy.path()).unwrap();
    store.restore(&manifest).unwrap();
    store.sync().unwrap();
    assert_eq!(History::read(&store).unwrap().summary().unwrap(), before);
    assert_eq!(
        LegacyHistory::read(&store).unwrap().summary().unwrap(),
        legacy
    );
}

#[test]
fn both_checkers_reject_bad_graphs_and_valid_state_recovers() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let a = forecast(&store);
    let o = observe(&store, &a.id, ResultKind::Failure, "Observed failure.").unwrap();
    let b = forecast(&store);
    let good = History::read(&store).unwrap();
    let mut bads = Vec::new();
    let mut bad = good.clone();
    bad.predictions.remove(&a.id);
    bads.push(bad);
    let mut bad = good.clone();
    bad.observations.remove(&o.id);
    bads.push(bad);
    let mut bad = good.clone();
    bad.observations.get_mut(&o.id).unwrap().at_ms = b.at_ms + 1;
    bads.push(bad);
    let mut bad = good.clone();
    bad.predictions
        .get_mut(&b.id)
        .unwrap()
        .forecast
        .scope
        .source = "other".into();
    bads.push(bad);
    let mut bad = good.clone();
    bad.predictions.get_mut(&a.id).unwrap().forecast.basis = vec![o.id.clone()];
    bads.push(bad);
    for bad in bads {
        assert!(bad.graph_invalid());
        let error = bad.audit().unwrap_err();
        assert!(!error.contains("disagree"), "{error}");
    }
    let mut bad = good.clone();
    bad.predictions
        .get_mut(&b.id)
        .unwrap()
        .forecast
        .probability_ppm = 1_000_000;
    assert!(bad.audit().unwrap_err().contains("estimate"));
    good.audit().unwrap();
}

#[test]
fn unknown_identity_is_session_scoped_and_inconclusive_is_not_training() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let mut g = group();
    g.conditions.model_identity = ModelIdentity::Unavailable {
        session_digest: session_digest("codex", "one"),
    };
    let p = predict(&store, g.clone(), "one", "Fixture.", BTreeMap::new()).unwrap();
    observe(&store, &p.id, ResultKind::Inconclusive, "Cancelled.").unwrap();
    assert_eq!(
        predict(&store, g.clone(), "one", "Fixture.", BTreeMap::new())
            .unwrap()
            .probability_ppm,
        500_000
    );
    assert!(predict(&store, g.clone(), "two", "Fixture.", BTreeMap::new()).is_err());
    g.conditions.model_identity = ModelIdentity::Unavailable {
        session_digest: session_digest("codex", "two"),
    };
    assert_eq!(
        predict(&store, g, "two", "Fixture.", BTreeMap::new())
            .unwrap()
            .probability_ppm,
        500_000
    );
}

#[test]
fn structured_cli_preserves_legacy_calls_and_rejects_native_observations() {
    let root = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let args = |op: &str| {
        vec![
            op.into(),
            "--root".into(),
            root.path().to_str().unwrap().into(),
            "--stdin".into(),
        ]
    };
    let call = |op: &str, v: Value| super::super::command(&args(op), &v.to_string());
    let input = json!({"source":"codex","session":"one","capability":"read","expectation":"The read succeeds.",
        "conditions":{"workspace":workspace.path(),"method":"file-read-v1","model_identity":{"kind":"known","name":"fixture","revision":null},
        "implementation":"executor-1","environment":"fixture"},"metadata":{"date":"one"}});
    let p = call("predict", input.clone()).unwrap();
    assert_eq!(p["grouping_version"], 2);
    call(
        "observe",
        json!({"prediction":p["id"],"result":"success","evidence":"Observed."}),
    )
    .unwrap();
    let mut next = input.clone();
    next["session"] = json!("two");
    next["metadata"] = json!({"date":"two","note":"incidental"});
    assert_eq!(call("predict", next).unwrap()["probability_ppm"], 666_666);
    let mut ambiguous = input;
    ambiguous["context"] = json!("old-style");
    assert!(call("predict", ambiguous).is_err());
    let old=call("predict",json!({"source":"codex","session":"one","context":"legacy","capability":"read","expectation":"Legacy fixture."})).unwrap();
    call(
        "observe",
        json!({"prediction":old["id"],"result":"success","evidence":"Legacy observation."}),
    )
    .unwrap();
    let native = {
        let store = Store::open(root.path()).unwrap();
        let mut g = group();
        g.measurement = Measurement::Runtime;
        predict(&store, g, "one", "Native fixture.", BTreeMap::new()).unwrap()
    };
    assert!(
        call(
            "observe",
            json!({"prediction":native.id,"result":"success","evidence":"An external claim."})
        )
        .is_err()
    );
    let status = call("status", json!({})).unwrap();
    assert_eq!(status["predictions"], 1);
    assert_eq!(status["grouped"]["predictions"], 3);
}

#[test]
fn artifact_changes_require_a_new_preview_and_review_metadata_cannot_authorize() {
    let root = tempfile::tempdir().unwrap();
    let support = tempfile::NamedTempFile::new().unwrap();
    let store = Store::open(root.path()).unwrap();
    std::fs::write(support.path(), "original supporting evidence").unwrap();
    let mut proposal = proposal(&store, group());
    proposal.candidates[0].support = vec![Evidence::File {
        path: support.path().into(),
        sha256: digest(b"original supporting evidence"),
    }];
    let p = preview(&store, proposal.clone()).unwrap();
    assert!(apply(&store, p.clone(), "unapproved", reviewer()).is_err());
    std::fs::write(support.path(), "changed").unwrap();
    assert!(
        apply(&store, p.clone(), &p.digest, reviewer())
            .unwrap_err()
            .contains("stale")
    );
    std::fs::remove_file(support.path()).unwrap();
    assert!(!preview(&store, proposal).unwrap().excluded.is_empty());
    assert!(History::read(&store).unwrap().reviews.is_empty());
}

#[test]
fn reviewed_graph_rejects_missing_wrong_and_revoked_memberships() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let proposal = proposal(&store, group());
    let preview = preview(&store, proposal).unwrap();
    let r = apply(&store, preview.clone(), &preview.digest, reviewer()).unwrap();
    let p = forecast(&store);
    let revoked = revoke(&store, &r.id, reviewer(), "Correction.").unwrap();
    let good = History::read(&store).unwrap();
    good.audit().unwrap();
    let mut cases = Vec::new();
    let mut bad = good.clone();
    bad.predictions.get_mut(&p.id).unwrap().review_basis.clear();
    cases.push(bad);
    let mut bad = good.clone();
    *bad.predictions
        .get_mut(&p.id)
        .unwrap()
        .review_basis
        .values_mut()
        .next()
        .unwrap() = "missing-review".into();
    cases.push(bad);
    let mut bad = good.clone();
    let forged = bad.predictions.get_mut(&p.id).unwrap();
    forged.review_state.push(revoked.id.clone());
    forged.review_state.sort();
    forged.forecast.at_ms = revoked.at_ms + 1;
    cases.push(bad);
    for bad in cases {
        assert!(bad.graph_invalid());
        assert!(bad.audit().is_err());
        let engine = NibliEngine::new();
        engine
            .assert_text(&format!("{SPEC}\n{}", bad.facts()))
            .unwrap();
        assert_eq!(
            engine.query_holds("sl_invalid(History).").unwrap(),
            QueryResult::True
        );
    }
}

#[test]
fn the_window_is_bounded_and_metadata_has_no_effect_on_ordering() {
    // Construct a valid archive in memory to avoid rerunning a full graph after
    // each fixture insertion. One complete audit checks the resulting graph.
    let mut h = History::default();
    for i in 0..35 {
        let id = format!("fixture-{i:02}");
        let at = 100 + i * 2;
        let p = Prediction {
            forecast: super::super::Prediction {
                id: id.clone(),
                at_ms: at,
                scope: group().scope(),
                session: "fixture".into(),
                expectation: "Fixture.".into(),
                model: MODEL.into(),
                checker: digest(SPEC.as_bytes()),
                basis: vec![],
                probability_ppm: 500_000,
                policy: super::super::Policy::Direct,
            },
            conditions: group().conditions,
            metadata: BTreeMap::new(),
            review_state: vec![],
            review_basis: BTreeMap::new(),
        };
        let o = Observation {
            id: format!("outcome-{id}"),
            prediction: id.clone(),
            at_ms: at + 1,
            result: ResultKind::Failure,
            evidence: "Observed fixture failure.".into(),
        };
        h.predictions.insert(id, p);
        h.observations.insert(o.id.clone(), o);
    }
    h.audit().unwrap();
    let candidates = h.candidates(&group(), &[]);
    let window = h.window(&candidates);
    assert_eq!(window.len(), 32);
    assert_eq!(window[0], "outcome-fixture-34");
    assert_eq!(window[31], "outcome-fixture-03");
    assert_eq!(
        estimate(window.iter().map(|id| h.observation(id).unwrap())),
        (29_411, super::super::Policy::InspectFirst)
    );
}

#[test]
fn regroup_cli_preview_apply_and_revoke_keep_the_approval_explicit() {
    let root = tempfile::tempdir().unwrap();
    let proposal = {
        let store = Store::open(root.path()).unwrap();
        proposal(&store, group())
    };
    let call = |operation: &str, input: Value| {
        super::super::command(
            &[
                "regroup".into(),
                operation.into(),
                "--root".into(),
                root.path().to_str().unwrap().into(),
                "--stdin".into(),
            ],
            &input.to_string(),
        )
    };
    let p = call("preview", json!(proposal)).unwrap();
    assert!(p["excluded"].as_object().unwrap().is_empty());
    assert!(call("apply", json!({"preview":p,"reviewer":reviewer()})).is_err());
    let r = call(
        "apply",
        json!({"preview":p,"approved_preview_digest":p["digest"],"reviewer":reviewer()}),
    )
    .unwrap();
    call(
        "revoke",
        json!({"review":r["id"],"reviewer":reviewer(),"reason":"Fixture correction."}),
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let h = History::read(&store).unwrap();
    h.audit().unwrap();
    assert_eq!((h.reviews.len(), h.revocations.len()), (1, 1));
    assert_eq!(forecast(&store).probability_ppm, 500_000);
}
