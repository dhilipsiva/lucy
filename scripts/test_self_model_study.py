"""Mechanism tests only; these fixtures are never empirical S6a evidence."""
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest

spec = importlib.util.spec_from_file_location("self_model_study", Path(__file__).with_name("self-model-study.py"))
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)


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
        bundle = dict(schema=1, evaluator="fixture", evaluator_model="fixture", implementation="fixture", protocol="fixture")
        for phase, count in (("calibration", 10), ("held_out", 20)):
            bundle[phase] = [dict(id=f"{phase}-{family}-{i}", definition=dict(family=family),
                                 disguises=dict(a="fixture a", b="fixture b"), oracle=[dict(id="fixed")])
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
            if status.exists():
                self.assertEqual(status.read_text().split()[2], "Z")

    def test_controls_keep_factual_inputs_and_restore_frozen_calibration(self):
        class FixtureRunner(study.Runner):
            def one(self, case, disguise, arm, data, label):
                if arm == "calibration":
                    (data / "frozen-only").write_text("calibration")
                else:
                    self.checks.append((arm, (data / "frozen-only").exists(), (data / "held-out-leak").exists()))
                    (data / "held-out-leak").write_text(case["id"])
                (data / "trial.json").write_text("{}")
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
                                for arm, has_calibration, leak in runner.checks))


if __name__ == "__main__":
    unittest.main()
