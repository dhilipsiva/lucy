#!/usr/bin/env python3
"""Paired synthetic audit diagnostic. No model inference or policy activation.

The parent enforces a 30-minute process-tree deadline, including preparation and
readback. A worker reserves the last 30 seconds for finalization. Every trial is a
new test-executable process. Private originals never enter the public report.
"""
import argparse
import copy
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

import s6a_evidence as evidence
import s6a_readiness_fixtures as fixtures
import importlib.util

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("audit_runner", ROOT / "scripts/self-model-study.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
FAMILIES = ("verification", "retrieval")
HISTORIES = (0, 10, 20)
PAIRS = 5
SECONDS = 1800


def save(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_bytes(evidence.encoded(value))
    temporary.replace(path)


def file_hash(path, deadline):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            evidence.check_deadline(deadline)
            result.update(chunk)
    return result.hexdigest()


def semantic(original):
    """Compare complete receipt content, preserving evidence and action order.

    Only fresh IDs, their content hashes, session IDs and measured times vary.
    Budget limits, probabilities, policies, evidence hashes, source membership,
    action request digests and all older seed receipts remain significant.
    """
    native, oracle = original["trial"]["task"], original["oracle"]["task"]
    mapping = {native: "native-task", oracle: "oracle-task"}
    rows = list(original["journal"].values()) + original["oracle_journal"]
    for task in (native, oracle):
        current = sorted((r for r in rows if r["task"] == task), key=lambda r: r["sequence"])
        for row in current:
            mapping[row["id"]] = f"{mapping[task]}-entry-{row['sequence']}"
            event = row["event"]
            if event["kind"] == "begin":
                mapping[event["task"]["definition"]["session"]] = mapping[task] + "-session"
            if event["kind"] == "action_started":
                mapping[event["action"]] = f"{mapping[task]}-action-{row['sequence']}"

    def normalize(value, path=()):
        if isinstance(value, str):
            return mapping.get(value, value)
        if isinstance(value, list):
            return [normalize(v, path) for v in value]
        if isinstance(value, dict):
            return {mapping.get(k, k): normalize(v, (*path, k)) for k, v in value.items()
                    if k != "at_ms" and not (k in ("active_ms", "checking_ms", "native_active_ms")
                                             and (not path or path[-1] != "budget"))}
        return value

    # Hashes of fresh receipts encode the normalized incidental IDs/timings.
    trial = copy.deepcopy(original["trial"])
    trial.pop("journal_digest")
    trial.pop("oracle_digest")
    archive = copy.deepcopy(original["artifacts"])
    archive.pop("directory")
    # The independently replayed trace supplies the actual model tool sequence.
    actions = [r for r in original["visible_trace"] if r["kind"] == "reply"]
    if not actions:
        raise ValueError("model response sequence missing")
    return dict(trial=normalize(trial),
                journal=sorted((normalize(r) for r in rows), key=lambda r: (r["task"], r["sequence"])),
                artifacts=normalize(archive), model_responses=normalize(actions))


def replay_native(run, trial, data, label, case):
    # Entry hashes use Rust struct field order. Reuse the existing typed native
    # scorer rather than interpreting those hashes as canonical JSON hashes.
    target = run.base / ("score-" + label)
    target.mkdir(mode=0o700)
    study = dict(schema=2, protocol=run.info["protocol"], implementation=run.info["implementation"],
                 model=run.info["model"], evaluator="audit-performance-diagnostic", evaluator_model="scripted",
                 bundle_digest=evidence.digest(case), live=False, complete=False,
                 active_ms=max(trial["active_ms"], round((time.monotonic() - run.started) * 1000)),
                 calibration={}, trials=[trial])
    request = dict(study=study, originals=[str(data / "trial.json")], calibration_originals=[])
    save(target / "score-input.json", request)
    result = run.call(["self-model", "task", "study-score", "--root", str(target), "--stdin"],
                      request, run.environment(target), "score-" + label)
    if result["live"] or any(s["eligible"] for s in result["scopes"]):
        raise ValueError("diagnostic replay attempted activation")


def evaluate(samples, complete):
    cells = []
    for family in FAMILIES:
        for history in HISTORIES:
            rows = [s for s in samples if s["family"] == family and s["prior_tasks"] == history]
            by_mode = {mode: [r for r in rows if r["mode"] == mode] for mode in ("0", "1")}
            paired = all(len(by_mode[m]) == PAIRS and {r["pair"] for r in by_mode[m]} == set(range(PAIRS))
                         for m in by_mode)
            equivalent = paired and all(next(r for r in by_mode["0"] if r["pair"] == p)["semantic_sha256"] ==
                                        next(r for r in by_mode["1"] if r["pair"] == p)["semantic_sha256"]
                                        for p in range(PAIRS))
            healthy = all(r["healthy"] for r in rows)
            medians = {m: {key: statistics.median(r[key] for r in data)
                           for key in ("native_wall_ms", "nibli_us", "nibli_evaluations", "hits", "misses", "evictions")}
                       for m, data in by_mode.items() if data}
            ratios = {key: medians["1"][key] / medians["0"][key]
                      for key in ("native_wall_ms", "nibli_us")
                      if len(medians) == 2 and medians["0"][key] > 0}
            speed = len(ratios) == 2 and (ratios["native_wall_ms"] <= 0.75 and ratios["nibli_us"] <= 0.50
                                                       if history == 20 else ratios["native_wall_ms"] <= 1.10)
            cells.append(dict(family=family, prior_tasks=history, samples=len(rows), paired=paired,
                              equivalent=equivalent, healthy=healthy, medians=medians, ratios=ratios,
                              passed=complete and paired and equivalent and healthy and speed))
    return dict(cells=cells, passed=complete and all(c["passed"] for c in cells))


def run_worker(binary, directory, started):
    final_deadline, deadline = started + SECONDS, started + SECONDS - 30
    report = dict(schema=1, kind="engineering_diagnostic", started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  ceiling_ms=SECONDS * 1000, complete=False, passed=False, samples=[], seeds=[],
                  cpu_affinity=sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
                  prior_task_definition="total task journals: half native scripted tasks, half their independent oracle tasks",
                  timing="native_wall_ms includes the fresh bridge/test process and native trial; Nibli spans count actual evaluations only",
                  normalization="fresh task/entry/action/session identities and measured times; receipt hashes replaced by normalized content",
                  registration="f04201d", base="d61d896", default_enabled=False)
    with tempfile.TemporaryDirectory(prefix="lucy-audit-bridge-") as bridge:
        bridge = Path(bridge)
        wrapper = bridge / "scripted-model"
        wrapper.write_text("#!" + sys.executable + "\nimport sys\nsys.path.insert(0, " + repr(str(ROOT / "scripts")) +
                           ")\nfrom s6a_scripted_bridge import main\nraise SystemExit(main())\n")
        wrapper.chmod(0o700)
        env = dict(os.environ, LUCY_READINESS_TEST_BINARY=str(binary), LUCY_READINESS_BRIDGE_ROOT=str(bridge), LUCY_AUDIT_CACHE="1")
        run = runner.Runner(wrapper, directory, SECONDS, started, diagnostic=True, base_env=env)
        run.deadline = deadline
        report.update(implementation=run.info["implementation"], protocol=run.info["protocol"], model=run.info["model"],
                      implementation_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                      runner_sha256=file_hash(Path(__file__), deadline), binary_sha256=file_hash(binary, deadline))
        save(run.base / "evidence.json", report)

        def checkpoint():
            report.update(evaluate(report["samples"], False))
            report["elapsed_ms"] = round((time.monotonic() - started) * 1000)
            save(run.base / "evidence.json", report)

        def copy_tree(source, target):
            def one(source, target):
                evidence.check_deadline(deadline)
                return shutil.copy2(source, target)
            shutil.copytree(source, target, copy_function=one)

        try:
            for family in FAMILIES:
                calibration = run.base / (family + "-calibration")
                calibration.mkdir(mode=0o700)
                for history in HISTORIES:
                    # Each scripted calibration creates one native and one
                    # independent oracle task. Count both, never relabel them.
                    for i in range(max(0, history // 2 - 5), history // 2):
                        run.base_env["LUCY_AUDIT_CACHE"] = "1"
                        run.one(fixtures.case(family, "calibration", i), "a", "calibration", calibration,
                                f"seed-{family}-{i}")
                    frozen = run.base / f"seed-{family}-{history}"
                    copy_tree(calibration, frozen)
                    for name in ("trial.json", "visible-trace.json", "profile.json"):
                        (frozen / name).unlink(missing_ok=True)
                    hashes = {str(p.relative_to(frozen)): file_hash(p, deadline)
                              for p in sorted(frozen.rglob("*")) if p.is_file()}
                    if history:
                        original = json.loads((calibration / "trial.json").read_bytes())
                        rows = list(original["journal"].values()) + original["oracle_journal"]
                        if len({r["task"] for r in rows}) != history:
                            raise ValueError("seed task count differs from registered history size")
                    report["seeds"].append(dict(family=family, prior_tasks=history, files=len(hashes), sha256=evidence.digest(hashes)))
                    checkpoint()
                    for pair in range(PAIRS):
                        for mode in (("0", "1") if pair % 2 == 0 else ("1", "0")):
                            evidence.check_deadline(deadline)
                            label = f"{family}-{history}-{pair}-{mode}"
                            data = run.base / label
                            copy_tree(frozen, data)
                            restored = {str(p.relative_to(data)): file_hash(p, deadline)
                                        for p in sorted(data.rglob("*")) if p.is_file()}
                            if hashes != restored:
                                raise ValueError("seed restore hash mismatch")
                            run.base_env["LUCY_AUDIT_CACHE"] = mode
                            case = fixtures.case(family, "diagnostic", 50)
                            save(run.base / (label + "-case.json"), case)
                            report["in_progress"] = dict(family=family, prior_tasks=history, pair=pair, mode=mode)
                            checkpoint()
                            trial = run.one(case, "a", "full", data, label)
                            original = json.loads((data / "trial.json").read_bytes())
                            profile = json.loads((data / "profile.json").read_bytes())
                            counts = profile["counts"]
                            sample = dict(family=family, prior_tasks=history, pair=pair, mode=mode,
                                native_wall_ms=next(t["ms"] for t in run.timings if t["label"] == label and t["stage"] == "native_trial"),
                                native_active_ms=original["native_active_ms"], nibli_us=profile["stages"]["nibli_audit"]["total_us"],
                                nibli_evaluations=counts["nibli_evaluations"], hits=counts.get("audit_cache_hits", 0),
                                misses=counts.get("audit_cache_misses", 0), evictions=counts.get("audit_cache_evictions", 0),
                                semantic_sha256=None, originals_sha256=file_hash(data / "trial.json", deadline), healthy=False)
                            report["samples"].append(sample)
                            checkpoint()
                            replay_native(run, trial, data, label, case)
                            normalized = semantic(original)
                            save(data / "semantic.json", normalized)
                            sample.update(semantic_sha256=evidence.digest(normalized),
                                healthy=profile["complete"] and trial["completed"] and not any(trial[k] for k in ("inconclusive", "false_verified", "budget_violation")))
                            report.pop("in_progress")
                            checkpoint()
            report["complete"] = True
        except (TimeoutError, RuntimeError, OSError, ValueError, KeyError, TypeError) as error:
            save(run.base / "private-error.json", dict(type=type(error).__name__, detail=str(error)))
            report["stop_reason"] = type(error).__name__
            if report.get("in_progress") and getattr(run, "timings", None):
                report["last_native_call"] = next((t for t in reversed(run.timings) if t["stage"] == "native_trial"), None)
        # Read back every private artifact within the same registered ceiling.
        try:
            hashes = {str(p.relative_to(run.base)): file_hash(p, final_deadline)
                      for p in sorted(run.base.rglob("*")) if p.is_file() and p.name != "evidence.json"}
            save(run.base / "preserved-hashes.json", hashes)
            for name, digest in hashes.items():
                if file_hash(run.base / name, final_deadline) != digest:
                    raise ValueError("private readback mismatch")
            report["preservation"] = dict(verified=True, files=len(hashes), manifest_sha256=evidence.digest(hashes))
        except (TimeoutError, OSError, ValueError) as error:
            report.update(complete=False, preservation=dict(verified=False), stop_reason=type(error).__name__)
        report["elapsed_ms"] = round((time.monotonic() - started) * 1000)
        report["complete"] &= time.monotonic() < final_deadline
        report.update(evaluate(report["samples"], report["complete"]))
        save(run.base / "evidence.json", report)
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--worker-start", type=float, help=argparse.SUPPRESS)
    args = parser.parse_args()
    os.umask(0o077)
    if args.worker_start is not None:
        result = run_worker(args.test_binary.resolve(strict=True), args.directory, args.worker_start)
        return 0 if result["passed"] else 2
    started = time.monotonic()
    if args.directory.exists():
        parser.error("directory must be new; never overwrite an attempt")
    process = subprocess.Popen([sys.executable, __file__, "--test-binary", str(args.test_binary.resolve(strict=True)),
                                "--directory", str(args.directory), "--worker-start", str(started)], start_new_session=True)
    try:
        process.wait(timeout=max(0.001, SECONDS - (time.monotonic() - started)))
    except subprocess.TimeoutExpired:
        runner.stop_trial(process)
        process.wait()
        # The last checkpoint and every partial original remain available.
        path = args.directory / "evidence.json"
        report = json.loads(path.read_bytes()) if path.exists() else dict(schema=1, samples=[])
        report.update(complete=False, passed=False, stop_reason="hard runtime ceiling", ceiling_ms=SECONDS * 1000,
                      elapsed_ms=round((time.monotonic() - started) * 1000))
        if args.directory.is_dir():
            save(path, report)
        return 2
    return process.returncode


if __name__ == "__main__":
    raise SystemExit(main())
