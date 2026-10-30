use nibli_kr::reify::InputIntent;
use nibli_verify::static_check::{session, verdict};
#[test]
fn moved_constitution_and_evidence_model_pass_the_engine_static_checker() {
    for (name, text) in [
        ("constitution", include_str!("../constitution/lucy.nibli")),
        (
            "empirical self-model",
            include_str!("../constitution/self_model.nibli"),
        ),
        (
            "task evidence",
            include_str!("../constitution/task_model.nibli"),
        ),
        (
            "grouped self-model",
            include_str!("../constitution/self_model_v2.nibli"),
        ),
    ] {
        let checker = session(text, InputIntent::Assertion).unwrap();
        assert!(
            verdict(&checker, "checker_complete(\"program:0\").").unwrap(),
            "{name}"
        );
        assert!(
            verdict(&checker, "checker_accepts(\"program:0\").").unwrap(),
            "{name}"
        );
        assert!(
            !verdict(
                &checker,
                "advisory_kind(\"program:0\", \"range_restriction\")."
            )
            .unwrap(),
            "{name}"
        );
        assert!(
            !verdict(
                &checker,
                "advisory_kind(\"program:0\", \"schema_conflict\")."
            )
            .unwrap(),
            "{name}"
        );
    }
}

#[test]
fn both_copies_of_the_constitution_are_identical() {
    assert_eq!(
        include_str!("../../lucy/constitution.nibli"),
        include_str!("../constitution/lucy.nibli"),
        "lucy/constitution.nibli and lucy-cli/constitution/lucy.nibli are edited together"
    );
}
