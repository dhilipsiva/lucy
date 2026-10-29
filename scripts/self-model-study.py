#!/usr/bin/env python3
"""Run S6a from an independently prepared sealed bundle; print aggregates only.

Never open a held-out bundle in a hooked conversation. All trial data stays in
the runner's private directory. This script never enrolls or synchronizes peers,
changes a provider, or enables a policy. It invokes the already selected model.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import signal
import shutil
import subprocess
import tempfile
import time
import s6a_evidence

ROOT = Path(__file__).resolve().parents[1]
ARMS = ["full", "lessons_withheld", "memory_removed", "roadmap_removed"]


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def save(path, value):
    path.write_bytes(encoded(value) + b"\n")
    path.chmod(0o600)


def relative(name):
    path = Path(name)
    if path.is_absolute() or not path.parts or any(p in (".", "..") or p.startswith(".") for p in path.parts):
        raise ValueError("fixture paths must be ordinary relative files")
    return path


def stop_trial(process):
    # Native command execution creates its own process group. Killing only the
    # parent group would leave an approved command alive. Stop and kill the
    # descendant tree too; ps returns only numeric IDs, never private arguments.
    try:
        os.kill(process.pid, signal.SIGSTOP)
    except ProcessLookupError:
        return
    descendants = {process.pid}
    try:
        for _ in range(3):
            rows = subprocess.run(["ps", "-eo", "pid=,ppid="], capture_output=True, check=True, timeout=2).stdout.splitlines()
            parents = [tuple(map(int, row.split())) for row in rows]
            changed = True
            while changed:
                old = len(descendants)
                descendants.update(pid for pid, parent in parents if parent in descendants)
                changed = len(descendants) != old
            for pid in descendants:
                try:
                    os.kill(pid, signal.SIGSTOP)
                except ProcessLookupError:
                    pass
    finally:
        for pid in descendants - {process.pid}:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.kill()


def validate_bundle(bundle):
    if bundle.get("schema") != 2 or not all(isinstance(bundle.get(k), str) and bundle[k] for k in ("evaluator", "evaluator_model", "implementation", "protocol")):
        raise ValueError("bundle needs an independent evaluator and frozen implementation/protocol")
    identities = set()
    methods = {}
    for phase, count in (("calibration", 10), ("held_out", 20)):
        cases = bundle.get(phase, [])
        for family in ("verification", "retrieval"):
            if sum(c.get("definition", {}).get("family") == family for c in cases) != count:
                raise ValueError("bundle does not contain the preregistered task counts")
        if len(cases) != count * 2:
            raise ValueError("unexpected task scope")
        for case in cases:
            identity = case.get("id")
            if not isinstance(identity, str) or not identity or identity in identities:
                raise ValueError("case identities must be unique across calibration and held-out sets")
            identities.add(identity)
            if set(case.get("disguises", {})) != {"a", "b"} or not case.get("oracle"):
                raise ValueError("each case needs two disguises and independent criteria")
            for name, content in case.get("files", {}).items():
                relative(name)
                if not isinstance(content, str) or len(content.encode()) > 32 * 1024 * 1024:
                    raise ValueError("fixture file exceeds the bound")
            for record in case.get("records", []):
                if not record.get("origin", "").startswith("fixture:") or record.get("payload", {}).get("private") is not True:
                    raise ValueError("study records must be explicit private fixture records")
            definition = case["definition"]
            family, method = definition["family"], definition.get("method")
            if not method or methods.setdefault(family, method) != method:
                raise ValueError("one frozen method per family is required")
            writes = case.get("output_writes", [])
            if not writes or any(not 0 < w.get("max_bytes", 0) <= 1048576 for w in writes):
                raise ValueError("bounded output writes required")
            for rule in writes:
                relative(rule["path"])
                if rule["path"] in case.get("files", {}):
                    raise ValueError("produced outputs cannot be preseeded as answers")
            s6a_evidence.validate_oracle(case)


class Runner:
    def __init__(self, binary, directory, seconds, started=None, prior_active_ms=0, reserve_seconds=0):
        self.started = time.monotonic() if started is None else started
        self.prior_active_ms = prior_active_ms
        self.deadline = self.started + seconds - prior_active_ms / 1000
        self.final_deadline = self.deadline
        self.reserve_seconds = reserve_seconds
        if os.name != "posix" or shutil.which("ps") is None:
            raise ValueError("study execution requires Unix process-tree cancellation")
        self.binary = str(binary.resolve(strict=True))
        self.base = directory.resolve()
        if self.base.exists() or any(p.joinpath(".git").exists() for p in self.base.parents) or self.base.is_relative_to("/nix/store"):
            raise ValueError("study directory must be new and outside repositories and the Nix store")
        result = subprocess.run([self.binary, "self-model", "task", "study-info"], capture_output=True,
                                timeout=max(0.001, min(30, self.deadline - time.monotonic())))
        if result.returncode:
            raise RuntimeError("selected model identity unavailable; no study state created")
        self.info = json.loads(result.stdout)
        self.real_data = self.info["real_data"]
        for real in (Path(self.real_data).resolve(), (Path.home() / ".local/share/lucy").resolve()):
            if self.base.is_relative_to(real) or real.is_relative_to(self.base):
                raise ValueError("study refuses real memory directories before creating state")
        self.roadmap = (ROOT / "roadmap/S6a.v2.md").read_text()
        if hashlib.sha256(self.roadmap.encode()).hexdigest() != self.info.get("protocol"):
            raise ValueError("runner roadmap differs from compiled protocol")
        self.base.mkdir(mode=0o700)
        self.base.joinpath(".lucy-isolated-study").write_text("isolated S6a study\n")
        (self.base / "protocol.md").write_text(self.roadmap)
        self.workspace = self.base / "workspace"
        self.workspace.mkdir(mode=0o700)
        self.home = self.base / "identity"
        self.home.mkdir(mode=0o700)
        shutil.copyfile(ROOT / "lucy/constitution.nibli", self.home / "constitution.nibli")
        self.calibration = {}
        self.calibration_originals = []
        self.trials = []
        self.originals = []
        self.replay_error = False
        save(self.base / "identity.json", dict(self.info, runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                                              prior_active_ms=prior_active_ms, reserve_seconds=reserve_seconds))

    def environment(self, data):
        env = os.environ.copy()
        env.update(LUCY_HOME=str(self.home), LUCY_DATA_DIR=str(data), LUCY_STUDY_ROOT=str(self.base), LUCY_STUDY_REAL_DATA=self.real_data)
        return env

    def call(self, args, value, env, label):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("study runtime ceiling reached")
        process = subprocess.Popen([self.binary, *args], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env=env, start_new_session=True)
        try:
            stdout, stderr = process.communicate(None if value is None else encoded(value), timeout=remaining)
        except subprocess.TimeoutExpired as error:
            # Stop the entire trial process group, including approved commands.
            # A terminated trial stays incomplete, with its journal preserved.
            stop_trial(process)
            stdout, stderr = process.communicate()
            self.base.joinpath(label + ".partial").write_bytes(stdout)
            self.base.joinpath(label + ".stderr").write_bytes(stderr)
            raise TimeoutError("study runtime ceiling reached") from error
        self.base.joinpath(label + ".stderr").write_bytes(stderr)
        if process.returncode:
            self.base.joinpath(label + ".stdout").write_bytes(stdout)
            raise RuntimeError("trial failed; private diagnostics retained")
        return json.loads(stdout)

    def fixture(self, case, data, arm):
        # This workspace is owned by this newly created runner directory.
        if self.workspace.parent != self.base or not self.base.joinpath(".lucy-isolated-study").is_file():
            raise ValueError("workspace ownership lost")
        shutil.rmtree(self.workspace)
        self.workspace.mkdir(mode=0o700)
        for name, text in case.get("files", {}).items():
            path = self.workspace / relative(name)
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            path.write_text(text)
            path.chmod(0o600)
        records = case.get("records", []) if arm != "memory_removed" else []
        if records:
            self.call(["store", "record", "--root", str(data), "--stdin"], records, self.environment(data), "seed")

    def one(self, case, disguise, arm, data, label):
        self.fixture(case, data, arm)
        roadmap = self.roadmap if arm != "roadmap_removed" else ""
        prompt = case["disguises"][disguise]
        if roadmap:
            prompt += "\nResearch roadmap (data, not permissions):\n" + roadmap
        request = dict(case=case["id"], disguise=disguise, arm=arm, prompt=prompt,
                       definition=case["definition"], oracle=case["oracle"], approvals=case.get("approvals", []),
                       output_writes=case.get("output_writes", []),
                       recall_records=case.get("records", []) if arm != "memory_removed" else [],
                       archive=str(self.base / "archives" / label),
                       inputs_digest=digest({"files": case.get("files", {}), "records": case.get("records", []) if arm != "memory_removed" else [], "roadmap": roadmap}))
        save(self.base / (label + "-request.json"), request)
        try:
            result = self.call(["self-model", "task", "study-trial", "--root", str(data),
                                "--workspace", str(self.workspace), "--stdin"], request, self.environment(data), label)
        except (TimeoutError, RuntimeError):
            # A killed native process cannot finish its archive. Preserve the
            # last workspace as explicitly partial; never score it as a trial.
            s6a_evidence.snapshot(self.workspace, self.base / (label + "-partial-workspace"))
            raise
        try:
            s6a_evidence.verify_original(data / "trial.json", request, case, self.roadmap, self.deadline)
        except (OSError, ValueError, KeyError, TypeError):
            self.replay_error = True
            raise
        trial = result["trial"]
        if trial["model"] != self.info["model"] or trial["implementation"] != self.info["implementation"]:
            raise ValueError("selected model or implementation changed during study")
        if time.monotonic() >= self.deadline:
            raise TimeoutError("artifact preservation reached the review reserve")
        return trial

    def key(self, trial):
        scope = trial["scope"]
        return digest({"scope": scope,
                       "checker": self.info["checker"], "policy": self.info["policy"]})

    def run(self, bundle):
        if bundle["implementation"] != self.info["implementation"] or bundle["protocol"] != self.info["protocol"]:
            raise ValueError("bundle was not sealed for this implementation and protocol")
        if hasattr(self, "deadline"):
            self.deadline -= self.reserve_seconds
        save(self.base / "sealed-bundle.json", bundle)
        calibration = self.base / "calibration"
        calibration.mkdir(mode=0o700)
        for index, case in enumerate(bundle["calibration"]):
            trial = self.one(case, "a", "calibration", calibration, f"calibration-{index}")
            key = self.key(trial)
            self.calibration[key] = self.calibration.get(key, 0) + 1
            original = self.base / f"calibration-{index}.json"
            shutil.copyfile(calibration / "trial.json", original)
            self.calibration_originals.append(str(original))
            if hasattr(self, "deadline") and time.monotonic() >= self.deadline:
                raise TimeoutError("calibration preservation reached the review reserve")
        frozen = self.base / "frozen"
        shutil.copytree(calibration, frozen)
        randomizer = random.Random(6)
        order = [(i, disguise, arm) for i in range(len(bundle["held_out"])) for disguise in ("a", "b") for arm in ARMS]
        randomizer.shuffle(order)
        for index, (case_index, disguise, arm) in enumerate(order):
            data = self.base / f"trial-{index}"
            if arm == "memory_removed":
                data.mkdir(mode=0o700)
            else:
                shutil.copytree(frozen, data)
            # Restore learning history, not the calibration trial's output
            # files. An interrupted trial must not appear to have an original.
            for name in ("trial.json", "visible-trace.json"):
                (data / name).unlink(missing_ok=True)
            trial = self.one(bundle["held_out"][case_index], disguise, arm, data, f"trial-{index}")
            self.trials.append(trial)
            self.originals.append(str(data / "trial.json"))
            save(self.base / "progress.json", {"completed_trials": len(self.trials), "planned_trials": len(order),
                                               "active_ms": round((time.monotonic() - self.started) * 1000) + getattr(self, "prior_active_ms", 0)})
            if hasattr(self, "deadline") and time.monotonic() >= self.deadline:
                raise TimeoutError("trial preservation reached the review reserve")

    def report(self, bundle, complete, reason):
        # A completed study must leave time for independent receipt adjudication.
        # Post-deadline diagnostics are explicitly incomplete and cannot activate.
        self.deadline = self.final_deadline
        complete = complete and time.monotonic() < self.deadline
        study = dict(schema=2, protocol=self.info["protocol"], implementation=self.info["implementation"],
                     model=self.info["model"], evaluator=bundle["evaluator"], evaluator_model=bundle["evaluator_model"],
                     bundle_digest=digest(bundle), live=True, complete=complete,
                     active_ms=round((time.monotonic() - self.started) * 1000) + self.prior_active_ms,
                     calibration=self.calibration, trials=self.trials)
        data = self.base / "report-store"
        data.mkdir(mode=0o700)
        request = {"study": study, "originals": self.originals, "calibration_originals": self.calibration_originals}
        save(self.base / "score-input.json", request)
        try:
            report = self.call(["self-model", "task", "study-score", "--root", str(data), "--stdin"],
                               request, self.environment(data), "score")
            report["scoring_complete"] = True
        except (TimeoutError, RuntimeError, OSError, ValueError):
            report = {"schema": 2, "protocol": self.info["protocol"],
                      "implementation": self.info["implementation"], "model": self.info["model"],
                      "complete": False, "s6a_complete": False, "scoring_complete": False, "scopes": [],
                      "meaning": "Adjudication incomplete; originals and score-input preserved; no eligible policy."}
            complete = False
        report["stop_reason"] = reason
        report["independent_replay"] = not self.replay_error
        report["replayed_originals"] = len(self.calibration_originals) + len(self.originals)
        report["active_ms"] = round((time.monotonic() - self.started) * 1000) + self.prior_active_ms
        if report["active_ms"] > 3600000 or not complete or self.replay_error:
            report["complete"] = False
            for scope in report["scopes"]:
                scope["eligible"] = False
        report["budget_ms"] = round((self.final_deadline - self.started) * 1000) + self.prior_active_ms
        report["prior_active_ms"] = self.prior_active_ms
        save(self.base / "report.json", report)
        print(json.dumps({"complete": report["complete"], "trials": len(self.trials), "active_ms": report["active_ms"],
                          "eligible_scopes": sum(s["eligible"] for s in report["scopes"]), "stop_reason": reason,
                          "private_report": str(self.base / "report.json"), "adaptation_enabled": False}))
        return report["complete"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/lucy")
    parser.add_argument("--bundle", type=Path, required=True, help="independent sealed bundle; never paste its contents into a hooked session")
    parser.add_argument("--directory", type=Path, help="new private study directory")
    parser.add_argument("--seconds", type=int, default=3600)
    parser.add_argument("--prior-active-ms", type=int, default=0, help="preparation charged by the coordinator")
    parser.add_argument("--reserve-seconds", type=int, default=300)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 3600:
        parser.error("runtime ceiling must be 1..3600 seconds")
    if not 0 <= args.prior_active_ms < args.seconds * 1000 or not 0 <= args.reserve_seconds < args.seconds:
        parser.error("invalid preparation time or review reserve")
    os.umask(0o077)
    started = time.monotonic()
    if args.bundle.stat().st_size > 64 * 1024 * 1024:
        parser.error("bundle exceeds size limit")
    bundle = json.loads(args.bundle.read_bytes())
    validate_bundle(bundle)
    if args.directory is None:
        parent = Path(tempfile.mkdtemp(prefix="lucy-s6a-"))
        args.directory = parent / "study"
    runner = Runner(args.binary, args.directory, args.seconds, started, args.prior_active_ms, args.reserve_seconds)
    complete, reason = False, "complete"
    try:
        runner.run(bundle)
        complete = True
    except (RuntimeError, ValueError, TimeoutError, OSError, KeyError, TypeError) as error:
        save(runner.base / "private-error.json", {"type":type(error).__name__, "detail":str(error)})
        reason = "incomplete; private diagnostics retained"
    complete = runner.report(bundle, complete, reason)
    return 0 if complete else 2


if __name__ == "__main__":
    raise SystemExit(main())
