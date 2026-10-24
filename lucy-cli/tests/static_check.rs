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
