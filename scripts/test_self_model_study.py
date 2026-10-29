"""Mechanism tests only; these fixtures are never empirical S6a evidence."""
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
import json
import copy
from unittest.mock import patch
import s6a_evidence

spec = importlib.util.spec_from_file_location("self_model_study", Path(__file__).with_name("self-model-study.py"))
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)
coord_spec = importlib.util.spec_from_file_location("s6a_coordinator", Path(__file__).with_name("s6a-coordinator.py"))
coordinator = importlib.util.module_from_spec(coord_spec)
coord_spec.loader.exec_module(coordinator)


class RunnerTests(unittest.TestCase):
    def test_real_memory_exclusion_happens_before_creating_trial_state(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            real = base / "real-memory"
            real.mkdir()
            binary = base / "fixture-cli"
            binary.write_text("#!/usr/bin/env python3\nimport json\nprint(json.dumps(" + repr({"real_data": str(real)}) + "))\n")
            binary.chmod(0o700)
            trial = real / "new-study"
            with self.assertRaisesRegex(ValueError, "real memory"):
                study.Runner(binary, trial, 5)
            self.assertFalse(trial.exists())

    def test_missing_and_reused_evaluator_cases_are_rejected(self):
        bundle = dict(schema=2, evaluator="fixture", evaluator_model="fixture", implementation="fixture", protocol="fixture")
        for phase, count in (("calibration", 10), ("held_out", 20)):
            bundle[phase] = [dict(id=f"{phase}-{family}-{i}", definition=dict(family=family, method="fixture", criteria=[]),
                                 disguises=dict(a="fixture a", b="fixture b"), output_writes=[dict(path="answer.json", max_bytes=1024)],
                                 oracle=[dict(id="source", check=dict(kind="json_equals", path="answer.json", pointer="/source", expected="fixture")),
                                         dict(id="answer", check=dict(kind="json_equals", path="answer.json", pointer="/answer", expected=42))])
                             for family in ("verification", "retrieval") for i in range(count)]
        study.validate_bundle(bundle)
        bundle["held_out"][0]["id"] = bundle["calibration"][0]["id"]
        with self.assertRaises(ValueError):
            study.validate_bundle(bundle)

    def test_timeout_stops_a_native_style_separate_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            script = base / "fixture.py"
            pidfile = base / "child.pid"
            script.write_text("import subprocess,os,time,sys\n"
                              "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],process_group=0)\n"
                              "open(sys.argv[1],'w').write(str(child.pid))\n"
                              "time.sleep(60)\n")
            runner = object.__new__(study.Runner)
            runner.binary, runner.base = sys.executable, base
            runner.deadline = time.monotonic() + 1
            with self.assertRaises(TimeoutError):
                runner.call([str(script), str(pidfile)], None, os.environ.copy(), "timeout")
            pid = int(pidfile.read_text())
            # A reparented zombie can remain briefly until init reaps it.
            status = Path(f"/proc/{pid}/stat")
            try:
                state = status.read_text().split()[2]
            except (FileNotFoundError, ProcessLookupError):
                state = None  # init reaped it between lookup and read.
            self.assertIn(state, (None, "Z"))

    def test_controls_keep_factual_inputs_and_restore_frozen_calibration(self):
        class FixtureRunner(study.Runner):
            def one(self, case, disguise, arm, data, label):
                if arm == "calibration":
                    (data / "frozen-only").write_text("calibration")
                else:
                    self.checks.append((arm, (data / "frozen-only").exists(), (data / "held-out-leak").exists(),
                                        (data / "trial.json").exists(), (data / "visible-trace.json").exists()))
                    (data / "held-out-leak").write_text(case["id"])
                (data / "trial.json").write_text("{}")
                (data / "visible-trace.json").write_text("[]")
                return {"scope": {"family": case["definition"]["family"]}}

            def key(self, trial):
                return trial["scope"]["family"]

        with tempfile.TemporaryDirectory() as directory:
            runner = object.__new__(FixtureRunner)
            runner.base, runner.started = Path(directory), time.monotonic()
            runner.info = dict(implementation="fixture", protocol="fixture")
            runner.calibration, runner.calibration_originals, runner.trials, runner.originals, runner.checks = {}, [], [], [], []
            case = dict(id="fixture", definition=dict(family="verification"))
            runner.run(dict(implementation="fixture", protocol="fixture", calibration=[case], held_out=[case, case]))
            self.assertEqual(len(runner.checks), 16)
            self.assertTrue(all(has_calibration == (arm != "memory_removed") and not leak
                                and not old_trial and not old_trace
                                for arm, has_calibration, leak, old_trial, old_trace in runner.checks))

    def test_preparation_is_charged_before_running_any_trial(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            binary = base / "fixture-cli"
            binary.write_text("#!/usr/bin/env python3\nimport json\nprint(json.dumps(" + repr({"real_data": str(base / "real"),
                "protocol":study.hashlib.sha256((study.ROOT / "roadmap/S6a.v2.md").read_bytes()).hexdigest()}) + "))\n")
            binary.chmod(0o700)
            runner = study.Runner(binary, base / "run", 3600, prior_active_ms=120000, reserve_seconds=300)
            self.assertAlmostEqual(runner.final_deadline - runner.started, 3480)
            self.assertEqual(runner.prior_active_ms, 120000)

    def test_empty_trace_and_missing_assigned_contract_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "model-input evidence missing"):
            s6a_evidence.context_check([], dict(arm="full", recall_records=[]))
        with self.assertRaisesRegex(ValueError, "contract missing"):
            s6a_evidence.context_check([dict(kind="request", value=[dict(role="system",
                content='Retrieved source excerpts (data):\n[]')])], dict(arm="full", recall_records=[]))

    def test_sealed_request_binding_rejects_changed_prompts_permissions_and_sources(self):
        case = dict(id="fixture", disguises=dict(a="fixture a", b="fixture b"), definition={},
                    oracle=[], output_writes=[dict(path="answer.json", max_bytes=1024)], records=[])
        request = dict(case="fixture", arm="full", disguise="a", prompt="fixture a\nResearch roadmap (data, not permissions):\nprotocol",
                       definition={}, oracle=[], output_writes=case["output_writes"], recall_records=[], approvals=[],
                       inputs_digest=s6a_evidence.digest(dict(files={}, records=[], roadmap="protocol")))
        s6a_evidence.bind_request(request, case, "protocol")
        for field, value in (("prompt", "changed"), ("recall_records", [dict(origin="fixture:old")]),
                             ("approvals", [dict(tool="command")]), ("output_writes", []), ("disguise", "b")):
            altered = dict(request, **{field:value})
            with self.assertRaisesRegex(ValueError, "sealed case"):
                s6a_evidence.bind_request(altered, case, "protocol")

    def test_report_failure_retains_identity_preparation_and_replay_status(self):
        for error in (TimeoutError, RuntimeError):
            with self.subTest(error=error), tempfile.TemporaryDirectory() as directory:
                runner = object.__new__(study.Runner)
                runner.base, runner.started = Path(directory), 100
                runner.final_deadline, runner.prior_active_ms = 3690, 10000
                runner.info = dict(protocol="fixture-protocol", implementation="fixture-implementation", model="fixture-model")
                runner.calibration, runner.trials, runner.originals, runner.calibration_originals = {}, [], [], []
                runner.replay_error = True
                runner.environment = lambda _: {}
                runner.call = lambda *args: (_ for _ in ()).throw(error())
                with patch.object(study.time, "monotonic", return_value=200):
                    runner.report(dict(evaluator="fixture", evaluator_model="fixture"), True, "fixture scoring failure")
                report = json.loads((runner.base / "report.json").read_bytes())
                self.assertEqual(report["active_ms"], 110000)
                self.assertFalse(report["independent_replay"])
                self.assertFalse(report["complete"])
                self.assertFalse(report["s6a_complete"])
                self.assertFalse(report["scoring_complete"])
                self.assertEqual(report["scopes"], [])
                for key, value in runner.info.items():
                    self.assertEqual(report[key], value)

    def test_preservation_may_not_consume_review_reserve(self):
        class FixtureRunner(study.Runner):
            def one(self, case, disguise, arm, data, label):
                (data / "trial.json").write_text("{}")
                return dict(scope=dict(family="verification"))
            def key(self, _):
                return "fixture"
        with tempfile.TemporaryDirectory() as directory:
            runner = object.__new__(FixtureRunner)
            runner.base, runner.started = Path(directory), 0
            runner.info = dict(implementation="fixture", protocol="fixture")
            runner.deadline, runner.reserve_seconds = 400, 300
            runner.calibration, runner.calibration_originals, runner.trials, runner.originals = {}, [], [], []
            with patch.object(study.time, "monotonic", return_value=101), self.assertRaisesRegex(TimeoutError, "preservation"):
                runner.run(dict(implementation="fixture", protocol="fixture", calibration=[dict(id="fixture")], held_out=[]))

    def test_coordinator_charges_shared_work_unlabelled_work_and_ledger_io(self):
        with tempfile.TemporaryDirectory() as directory:
            tick = [0.0]
            real_save = coordinator.save
            def slow_save(*args):
                real_save(*args)
                tick[0] += .01
            with patch.object(coordinator.time, "monotonic", side_effect=lambda:tick[0]), patch.object(coordinator, "save", side_effect=slow_save):
                clock = coordinator.Clock(Path(directory) / "clock.json")
                tick[0] += 1
                with clock.phase("one model", [coordinator.MODELS[0]]):
                    tick[0] += 2
                tick[0] += 1
                active = clock.active
                self.assertGreaterEqual(active[coordinator.MODELS[0]], 4020)
                self.assertGreaterEqual(active[coordinator.MODELS[1]], 2010)
                self.assertAlmostEqual(active[coordinator.MODELS[0]] - active[coordinator.MODELS[1]], 2010, delta=2)
                tick[0] += 3600
                with self.assertRaises(TimeoutError):
                    clock.remaining(coordinator.MODELS)

    def test_coordinator_rejects_real_memory_before_creating_anything(self):
        with tempfile.TemporaryDirectory() as directory:
            real = Path(directory) / "real"
            target = real / "new-study"
            with self.assertRaises(ValueError):
                coordinator.private_directory(target, real)
            self.assertFalse(target.exists())

    def test_review_requires_typed_gates_and_no_contradictory_approval(self):
        good = dict(accept_evidence=True, technical_qualification=True, disagreements=[], limitations=[], s6a_complete=False)
        coordinator.validate_review(good)
        for change in (dict(technical_qualification="true"), dict(disagreements=["unresolved"]),
                       dict(accept_evidence=False), dict(limitations="none"), dict(s6a_complete=True)):
            with self.assertRaises(ValueError):
                coordinator.validate_review(dict(good, **change))

    def test_changed_pins_and_expired_artifact_deadline_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "synthetic-binary"
            binary.write_bytes(b"fixture-v1")
            freeze = dict(binary_sha256=coordinator.file_hash(binary), files={})
            coordinator.check_pins(freeze, binary, time.monotonic() + 10)
            binary.write_bytes(b"fixture-v2")
            with self.assertRaisesRegex(ValueError, "executable changed"):
                coordinator.check_pins(freeze, binary, time.monotonic() + 10)
            with self.assertRaises(TimeoutError):
                coordinator.file_hash(binary, time.monotonic() - 1)
            with self.assertRaises(TimeoutError):
                s6a_evidence.manifest(Path(directory), time.monotonic() - 1)

    def test_independent_metrics_reject_agreeing_forged_trial_summaries(self):
        frozen = dict(scope={}, probability_ppm=500000, definition=dict(budget=dict(steps=12, active_ms=1000)))
        original = dict(native_active_ms=2, journal={"done":dict(task="fixture", event=dict(kind="finished", verdict="failure", active_ms=1))})
        original["trial"] = dict(task="fixture", scope={}, budget=frozen["definition"]["budget"], completed=False,
            false_verified=False, inconclusive=False, observed=False, probability_ppm=500000,
            checking_ms=1, budget_violation=False, active_ms=3)
        s6a_evidence.verify_trial_metrics(original, frozen, False)
        for key in ("completed", "observed", "false_verified"):
            bad = copy.deepcopy(original)
            bad["trial"][key] = True
            with self.assertRaisesRegex(ValueError, "native receipts"):
                s6a_evidence.verify_trial_metrics(bad, frozen, False)

    def test_replay_counts_cannot_hide_duplicate_or_wrong_phase_requests(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base / "protocol.md").write_text("fixture protocol")
            bundle = dict(protocol=study.hashlib.sha256(b"fixture protocol").hexdigest(),
                          calibration=[dict(id="cal")], held_out=[dict(id="held")])
            study.save(base / "sealed-bundle.json", bundle)
            request = dict(case="cal", disguise="a", arm="calibration")
            for i in range(2):
                study.save(base / f"calibration-{i}-request.json", request)
                study.save(base / f"calibration-{i}.json", {})
            with patch.object(s6a_evidence, "verify_original", return_value=dict(trial=dict(task="fixture"))):
                with self.assertRaisesRegex(ValueError, "duplicated trial request"):
                    s6a_evidence.verify_run(base)
            study.save(base / "calibration-0-request.json", dict(request, arm="full"))
            with self.assertRaisesRegex(ValueError, "unplanned"):
                s6a_evidence.verify_run(base)

    def test_independent_scoring_rejects_forged_gates_and_missing_populations(self):
        # Receipt-free synthetic score fixture tests arithmetic/bindings only.
        # It is explicitly injected as already replayed; never empirical evidence.
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            info = dict(model="fixture@pinned", protocol="p", implementation="i", checker="c", policy="fixture")
            bundle = dict(calibration=[], held_out=[])
            trials, calibration, originals, calibration_paths, scopes, verified = [], {}, [], [], [], {}
            def original(trial, path):
                study.save(path, dict(trial=trial))
                verified[str(path)] = study.hashlib.sha256(path.read_bytes()).hexdigest()
                return str(path)
            def metrics(rows):
                return dict(trials=len(rows), completed=0, false_verified=0, inconclusive=0, budget_violations=0,
                            active_ms=len(rows), checking_ms=0, brier=.25 if rows else None, scored=len(rows))
            for family in ("verification", "retrieval"):
                scope = dict(family=family, model=info["model"])
                key = s6a_evidence.digest(dict(scope=scope, checker=info["checker"], policy=info["policy"]))
                calibration[key] = 10
                template = dict(scope=scope, model=info["model"], criteria_digest="a"*64, inputs_digest="b"*64,
                                budget=dict(steps=12, active_ms=1000), completed=False, false_verified=False,
                                budget_violation=False, inconclusive=False, probability_ppm=500000, observed=False,
                                active_ms=1, checking_ms=0)
                for i in range(10):
                    case = f"calibration-{family}-{i}"
                    bundle["calibration"].append(dict(id=case))
                    calibration_paths.append(original(dict(template, case=case, disguise="a", arm="calibration"), base / (case + ".json")))
                for i in range(20):
                    case = f"held-{family}-{i}"
                    bundle["held_out"].append(dict(id=case))
                    for disguise in ("a", "b"):
                        for arm in study.ARMS:
                            trial = dict(template, case=case, disguise=disguise, arm=arm)
                            trials.append(trial)
                            originals.append(original(trial, base / f"{case}-{disguise}-{arm}.json"))
                rows = [row for row in trials if row["scope"] == scope]
                arms = []
                for arm in study.ARMS:
                    selected = [row for row in rows if row["arm"] == arm]
                    arms.append(dict(metrics(selected), arm=arm, by_disguise=[dict(metrics([r for r in selected if r["disguise"]==d]), disguise=d) for d in ("a", "b")]))
                scopes.append(dict(scope=key, family=family, complete=True, completion_delta=0, paired_ci95=[0, 0],
                                   eligible=False, forecast_qualifies=False, arms=arms))
            scoring = dict(study=dict(schema=2, bundle_digest=s6a_evidence.digest(bundle), trials=trials,
                            calibration=calibration, live=True, complete=True, **{k:info[k] for k in ("model", "protocol", "implementation")}),
                           originals=originals, calibration_originals=calibration_paths)
            report = dict(complete=True, active_ms=1000, scopes=scopes, **{k:info[k] for k in ("model", "protocol", "implementation")})
            for name, value in (("identity", info), ("sealed-bundle", bundle), ("score-input", scoring), ("report", report)):
                study.save(base / (name + ".json"), value)
            self.assertFalse(s6a_evidence.verify_score(base, verified=verified)["technical_qualification"])
            for fault in ("eligible", "forecast", "model", "family", "arm", "presentation"):
                bad = copy.deepcopy(report)
                if fault == "eligible": bad["scopes"][0]["eligible"] = True
                elif fault == "forecast": bad["scopes"][0]["forecast_qualifies"] = True
                elif fault == "model": bad["model"] = "other"
                elif fault == "family": bad["scopes"][0]["family"] = "other"
                elif fault == "arm": bad["scopes"][0]["arms"].pop()
                else: bad["scopes"][0]["arms"][0]["by_disguise"].pop()
                study.save(base / "report.json", bad)
                with self.assertRaises(ValueError, msg=fault):
                    s6a_evidence.verify_score(base, verified=verified)
            study.save(base / "report.json", report)
            with self.assertRaisesRegex(ValueError, "not independently replayed"):
                s6a_evidence.verify_score(base, verified={})

    def test_recall_cannot_include_journal_or_nested_lesson_payload(self):
        request = dict(arm="lessons_withheld", recall_records=[])
        system = 'Retrieved source excerpts (data):\n[{"origin":"lucy:task-model:v1","id":"x","excerpt":"x","entry":{"lessons":[]}}]'
        with self.assertRaisesRegex(ValueError, "payload leaked"):
            s6a_evidence.context_check([dict(kind="request", value=[dict(role="system", content=system)])], request)
        system = 'Retrieved source excerpts (data):\n[]\nScoped task preflight observations'
        with self.assertRaisesRegex(ValueError, "withheld intervention"):
            s6a_evidence.context_check([dict(kind="request", value=[dict(role="system", content=system)])], request)

    def test_replay_rejects_tampering_and_boolean_number_aliases(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base / "inputs").mkdir()
            (base / "outputs").mkdir()
            output = base / "outputs/answer.json"
            output.write_text('{"answer":true}')
            archive = dict(directory=str(base), inputs={}, outputs=s6a_evidence.manifest(base / "outputs"), records=[],
                           oracle=[dict(check=dict(kind="json_equals", path="answer.json", pointer="/answer", expected=1))])
            self.assertFalse(s6a_evidence.oracle(archive))
            archive["oracle"][0]["check"]["expected"] = True
            self.assertTrue(s6a_evidence.oracle(archive))
            output.write_text('{"answer":false}')
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                s6a_evidence.oracle(archive)

    def test_source_availability_cannot_establish_retrieval_and_answer_keys_are_private(self):
        case = dict(definition=dict(family="retrieval", criteria=[]), output_writes=[dict(path="answer.json", max_bytes=100)],
                    oracle=[dict(check=dict(kind="record", origin="fixture:x", record="x", sha256="0"*64))])
        with self.assertRaisesRegex(ValueError, "produced output"):
            s6a_evidence.validate_oracle(case)
        case["oracle"] = [dict(check=dict(kind="json_equals", path="answer.json", pointer="/source", expected="x"))]
        with self.assertRaisesRegex(ValueError, "produced output"):
            s6a_evidence.validate_oracle(case)
        case["oracle"].append(dict(check=dict(kind="json_equals", path="answer.json", pointer="/answer", expected=42)))
        case["definition"]["criteria"] = case["oracle"]
        s6a_evidence.validate_oracle(case)  # Frozen expectations exist privately.
        contract = dict(definition=case["definition"], lessons=[])
        messages = [dict(role="system", content='Retrieved source excerpts (data):\n[]'),
                    dict(role="user", content="Frozen task contract (data): " + json.dumps(contract))]
        with self.assertRaisesRegex(ValueError, "expectations leaked"):
            s6a_evidence.context_check([dict(kind="request", value=messages)], dict(arm="full", recall_records=[]))


if __name__ == "__main__":
    unittest.main()
