#!/usr/bin/env python3
"""Private engineering readiness, never empirical learning evidence or activation.

The default run is one scripted 20/320 matrix (30 minutes), followed by at most
eight synthetic trials per pinned local model (15 minutes each). No retries.
"""
import argparse
import datetime
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import urllib.request

import s6a_evidence as evidence
import s6a_readiness_fixtures as fixtures

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


runner = module("readiness_runner", "self-model-study.py")
coordinator = module("readiness_coordinator", "s6a-coordinator.py")
PINS = {
    "qwen3.6:35b-a3b-q4_K_M":"07d35212591fc27746f0a317c975a6d68754fb38e9053d82e25f06057af28522",
    "llama3.1:8b":"46e0c10c039e019119339687c3c1757cc81b9da49709a3b3924863ba87ca666e",
}


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def save(path, value):
    coordinator.save(path, value)


def preserve(base, deadline):
    hashes = {}
    for path in sorted(base.rglob("*")):
        evidence.check_deadline(deadline)
        if path.is_symlink():
            raise ValueError("symlink in readiness originals")
        if path.is_file() and path.name not in ("readiness.json", "preserved-hashes.json"):
            hashes[str(path.relative_to(base))] = coordinator.file_hash(path, deadline)
    save(base / "preserved-hashes.json", hashes)
    for name, digest in hashes.items():
        if coordinator.file_hash(base / name, deadline) != digest:
            raise ValueError("readiness original readback mismatch")
    return dict(status="verified locally", files=len(hashes), replication="none")


def replay(run, deadline):
    verified = {}
    with run.measure("independent_run_replay", "run"):
        originals = evidence.verify_run(run.base, deadline, verified)
    with run.measure("independent_score_replay", "score"):
        score = evidence.verify_score(run.base, deadline, verified)
    report = json.loads((run.base / "report.json").read_bytes())
    if report.get("live") is not False or any(scope["eligible"] for scope in report["scopes"]):
        raise ValueError("diagnostic evidence attempted qualification")
    # Use the production coordinator gate. The external review is explicitly
    # scripted, with no independent review or human approval claimed.
    review = dict(accept_evidence=True, technical_qualification=False,
                  disagreements=[], limitations=["engineering diagnostic"], s6a_complete=False)
    state = dict(replay=originals, score_replay=score)
    qualification = coordinator.technical_qualification([report], [state], review, [report["active_ms"]])
    return dict(replay=originals, score_replay=score, coordinator=coordinator.qualification_status(qualification))


def check_controls(run, deadline=None):
    """Inspect our public synthetic fixtures only, never prior sealed studies."""
    controls = dict(full=False, lessons_withheld=False, memory_removed=False, roadmap_removed=False)
    for path in run.base.glob("trial-*/trial.json"):
        evidence.check_deadline(deadline)
        original = json.loads(path.read_bytes())
        arm = original["trial"]["arm"]
        task = next(e["event"]["task"] for e in original["journal"].values()
                    if e["event"]["kind"] == "begin" and e["task"] == original["trial"]["task"])
        preflights = [e for e in original["journal"].values() if e["task"] == task["id"]
                      and e["event"].get("operation") == "lesson_preflight"]
        active = any(lesson["active"] for lesson in task["lessons"])
        if arm in ("full", "roadmap_removed"):
            if not active or not preflights:
                raise ValueError("synthetic lesson did not reach its assigned intervention")
        elif arm == "lessons_withheld":
            if not active or preflights:
                raise ValueError("withheld synthetic intervention was not isolated")
        elif arm == "memory_removed" and (active or preflights or task["basis"]):
            raise ValueError("synthetic memory removal retained learning history")
        controls[arm] = True
    return controls


def profile_summary(run):
    selected = {}
    labels = (("calibration-0", 0), ("calibration-10", 10), ("trial-0", 20)) if run.calibration else (("trial-0", 0), ("trial-4", 20))
    for label, history in labels:
        path = run.base / (label + "-profile.json")
        selected[str(history)] = json.loads(path.read_bytes()) if path.exists() else None
    trials = [row["ms"] for row in run.timings if row["stage"] == "native_trial"]
    trials.sort()
    return dict(history_sizes=selected, native_trials=dict(count=len(trials),
        p50_ms=trials[(len(trials)-1)//2] if trials else None,
        p95_ms=trials[min(len(trials)-1, (len(trials)*95+99)//100-1)] if trials else None),
        interpretation="inclusive nested timings; wall time alone is the budget authority")


def scripted_run(base, test_binary, seconds=1800, calibration=10, held_out=20):
    started = time.monotonic()
    deadline = started + seconds
    wrapper = base / "scripted-model"
    wrapper.write_text("#!/usr/bin/env python3\nimport sys\nsys.path.insert(0, " + repr(str(ROOT / "scripts")) + ")\n"
                       "from s6a_scripted_bridge import main\nraise SystemExit(main())\n")
    wrapper.chmod(0o700)
    env = dict(os.environ, LUCY_READINESS_TEST_BINARY=str(test_binary), LUCY_READINESS_BRIDGE_ROOT=str(base))
    run = runner.Runner(wrapper, base / "scripted", seconds, started, reserve_seconds=min(180, seconds/4),
                        diagnostic=True, base_env=env)
    bundle = fixtures.bundle(run.info, calibration, held_out)
    if calibration == 10 and held_out == 20:
        runner.validate_bundle(bundle)
    result = dict(kind="engineering_diagnostic", model_calls=False, planned_calibration=calibration*2,
                  planned_trials=held_out*16, mechanics_ready=False)
    complete = False
    try:
        run.run(bundle)
        complete = True
    except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
        save(run.base / "readiness-error.json", dict(type=type(error).__name__, detail=str(error)))
    try:
        run.final_deadline = deadline - min(90, seconds/8)
        run.report(bundle, complete, "synthetic engineering diagnostic")
        result.update(replay(run, deadline - 20))
        result["controls"] = check_controls(run, deadline - 20)
        result["mechanics_ready"] = (complete and result["replay"]["complete_originals"]
            and result["score_replay"]["valid"] and all(result["controls"].values()))
    except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
        save(run.base / "readiness-finalization-error.json", dict(type=type(error).__name__, detail=str(error)))
        result["finalization"] = "incomplete; private diagnostics preserved"
    result.update(calibration=sum(run.calibration.values()), trials=len(run.trials), profile=profile_summary(run))
    try:
        result["persistence"] = preserve(run.base, deadline)
    except (OSError, ValueError, TimeoutError):
        result["persistence"] = dict(status="incomplete readback", replication="none")
        result["mechanics_ready"] = False
    result["active_ms"] = round((time.monotonic()-started)*1000)
    if result["active_ms"] > seconds*1000:
        result["mechanics_ready"] = False
    save(run.base / "readiness.json", result)
    return result, run.base / "frozen"


def model_run(base, binary, model, frozen, seconds=900):
    started = time.monotonic()
    deadline = started + seconds
    result = dict(kind="engineering_diagnostic", model=model, model_calls=True,
                  planned_trials=8, model_ready=False, adaptation_enabled=False)
    run = None
    try:
        with urllib.request.urlopen("http://127.0.0.1:11434/api/tags", timeout=min(10, seconds)) as response:
            catalog = json.load(response)["models"]
        if not any(row["name"] == model and row["digest"].removeprefix("sha256:") == PINS[model] for row in catalog):
            raise ValueError("pinned local model unavailable; no download or substitution")
        env = dict(os.environ, LUCY_CHAT_MODEL=model, LUCY_CHAT_DIGEST=PINS[model])
        run = runner.Runner(binary, base, seconds, started, reserve_seconds=120, diagnostic=True, base_env=env)
        bundle = fixtures.bundle(run.info, calibration=0, held_out=2)
        if run.info["model"] != f"ollama:{model}@{PINS[model]}":
            raise ValueError("selected provider differs from pinned local model")
        save(run.base / "sealed-bundle.json", bundle)
        run.deadline = deadline - 120
        schedule = [(case, history) for history in (0, 20)
                    for family in ("verification", "retrieval")
                    for case in bundle["held_out"] if case["definition"]["family"] == family]
        save(run.base / "smoke-schedule.json", [dict(label=f"trial-{i}", case=case["id"], history=h,
             disguise="a" if h==0 else "b", arm="lessons_withheld") for i,(case,h) in enumerate(schedule)])
        for index, (case, history) in enumerate(schedule):
            evidence.check_deadline(run.deadline)
            data = run.base / f"trial-{index}"
            with run.measure("restore_history", f"trial-{index}"):
                if history:
                    shutil.copytree(frozen, data)
                else:
                    data.mkdir(mode=0o700)
                for name in ("trial.json", "visible-trace.json", "profile.json"):
                    (data / name).unlink(missing_ok=True)
            trial = run.one(case, "a" if history==0 else "b", "lessons_withheld", data, f"trial-{index}")
            run.trials.append(trial)
            run.originals.append(str(data / "trial.json"))
            save(run.base / "progress.json", dict(completed_trials=len(run.trials), planned_trials=8,
                 synthetic_history=history, elapsed_ms=round((time.monotonic()-started)*1000)))
    except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
        result["stop_reason"] = type(error).__name__
        if run:
            save(run.base / "readiness-error.json", dict(type=type(error).__name__, detail=str(error)))
    if run:
        try:
            run.final_deadline = deadline - 45
            # These eight smoke tasks never constitute the registered matrix.
            run.report(bundle, False, "synthetic model capability and timing only")
            result.update(replay(run, deadline - 20))
            result["model_ready"] = (len(run.trials) == 8 and result["score_replay"]["valid"]
                and not any(t["false_verified"] or t["budget_violation"] for t in run.trials)
                and all(any(t["completed"] and t["scope"]["family"] == family for t in run.trials)
                        for family in ("verification", "retrieval")))
        except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
            save(run.base / "readiness-finalization-error.json", dict(type=type(error).__name__, detail=str(error)))
            result["finalization"] = "incomplete; private diagnostics preserved"
        result.update(trials=len(run.trials), completed=sum(t["completed"] for t in run.trials),
            inconclusive=sum(t["inconclusive"] for t in run.trials),
            false_verified=sum(t["false_verified"] for t in run.trials),
            budget_violations=sum(t["budget_violation"] for t in run.trials), profile=profile_summary(run))
        try:
            result["persistence"] = preserve(run.base, deadline)
        except (OSError, ValueError, TimeoutError):
            result["persistence"] = dict(status="incomplete readback", replication="none")
            result["model_ready"] = False
    result["active_ms"] = round((time.monotonic()-started)*1000)
    if result["active_ms"] > seconds*1000:
        result["model_ready"] = False
    base.mkdir(mode=0o700, exist_ok=True)
    save(base / "readiness.json", result)
    return result


def build_test_binary():
    result = subprocess.run(["cargo", "test", "--release", "--lib", "-p", "lucy-cli", "--no-run", "--message-format=json"],
                            cwd=ROOT, capture_output=True, timeout=600, check=True)
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith(b"{")]
    return Path(next(row["executable"] for row in artifacts if row.get("executable")
                     and row.get("target", {}).get("name") == "lucy_cli"))


def render_report(result):
    """Readable aggregate report; never include prompts, traces, or private paths."""
    def minutes(value):
        return "unavailable" if value is None else f"{value/60000:.2f} min"
    lines = ["# S6a engineering readiness", "", "Synthetic engineering diagnostics; S6a remains incomplete.", "",
        "| Run | Preserved / planned trials | Verified successes | Elapsed | Readiness gate |",
        "| --- | ---: | ---: | ---: | --- |"]
    scripted = result.get("scripted", {})
    if scripted:
        lines.append(f"| Scripted matrix | {scripted.get('trials',0)} / {scripted.get('planned_trials',320)} | "
            f"not a model capability score | {minutes(scripted.get('active_ms'))} | "
            + ("passed" if scripted.get("mechanics_ready") else "not passed") + " |")
    for model, row in result.get("models", {}).items():
        lines.append(f"| {model} | {row.get('trials',0)} / {row.get('planned_trials',8)} | "
            f"{row.get('completed','unavailable')} | {minutes(row.get('active_ms'))} | "
            + ("passed" if row.get("model_ready") else "not passed") + " |")
    lines += ["", "Model smoke completeness is judged against eight scheduled tasks, not the full registered matrix. "
        "Its native study report therefore remains incomplete and non-qualifying even when the smoke gate passes.", "",
        "No independent evaluator, human approval, empirical learning benefit, or policy activation is claimed.", "",
        "## Evidence and gaps", ""]
    for name, row in [("Scripted matrix", scripted), *result.get("models", {}).items()]:
        if not row:
            continue
        original = row.get("replay", {})
        scoring = row.get("score_replay", {})
        persistence = row.get("persistence", {})
        lines.append(f"- {name}: {original.get('originals_verified','unavailable')} originals replayed; "
            f"original replay valid={original.get('valid','unavailable')}; score replay valid={scoring.get('valid','unavailable')}; "
            f"persistence={persistence.get('status','unavailable')} ({persistence.get('files','unavailable')} files).")
    if not scripted.get("mechanics_ready", False):
        lines.append("- The complete scripted matrix has not passed its readiness gate. Missing comparisons remain unavailable.")
    for model, row in result.get("models", {}).items():
        if not row.get("model_ready", False):
            lines.append(f"- {model}: {row.get('inconclusive','unavailable')} inconclusive outcomes; inspect the private synthetic traces before another study.")
    lines += ["", "## Timing samples", "", "Inclusive Nibli timings are nested within wall time; do not add them.", "",
        "| Prior synthetic calibration tasks | Native wall time | Nibli audit time | Audit calls |",
        "| ---: | ---: | ---: | ---: |"]
    for history, sample in scripted.get("profile", {}).get("history_sizes", {}).items():
        if sample:
            audit = sample.get("stages", {}).get("nibli_audit", {})
            lines.append(f"| {history} | {sample['wall_ms']/1000:.3f} s | "
                f"{audit.get('total_us',0)/1000000:.3f} s | {audit.get('calls','unavailable')} |")
    lines += ["", f"Provider unchanged: {result.get('provider_unchanged','unavailable')}. "
        f"Frozen implementation unchanged during the run: {result.get('frozen_implementation_unchanged','unavailable')}.", "",
        "Originals remain local. No replication or publication was attempted. See each run's preserved-hashes.json "
        "for readback evidence. Future empirical work requires a fresh registration; do not treat these diagnostics as held-out results.", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--scripted-binary", type=Path, help="prebuilt release unit-test executable")
    args = parser.parse_args()
    os.umask(0o077)
    binary = args.binary.resolve(strict=True)
    real = os.environ.get("LUCY_DATA_DIR", str(Path(os.environ.get("XDG_DATA_HOME", str(Path.home()/".local/share")))/"lucy"))
    base = coordinator.private_directory(args.directory, real)
    test_binary = args.scripted_binary.resolve(strict=True) if args.scripted_binary else build_test_binary()
    base.mkdir(parents=True, mode=0o700)
    provider = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home()/".config"))) / "lucy/providers.json"
    before = coordinator.file_hash(provider) if provider.exists() else None
    result = dict(schema=1, kind="engineering_diagnostic", started_at=now(), s6a_complete=False,
        adaptation_enabled=False, publication="none", replication="none", models={})
    freeze = dict(source_commit=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
        source_dirty=bool(subprocess.check_output(["git","status","--porcelain"],cwd=ROOT)),
        binary_sha256=coordinator.file_hash(binary), test_binary_sha256=coordinator.file_hash(test_binary),
        files={str(path.relative_to(ROOT)):coordinator.file_hash(path) for path in
            [*sorted((ROOT/"scripts").glob("*s6a*py")), ROOT/"scripts/self-model-study.py", ROOT/"roadmap/S6a.v2.md", ROOT/"lucy/constitution.nibli"]},
        models=PINS)
    save(base / "freeze.json", freeze)
    try:
        result["scripted"], frozen = scripted_run(base, test_binary)
        save(base / "readiness.json", result)
        print(json.dumps(dict(phase="scripted finished", mechanics_ready=result["scripted"]["mechanics_ready"],
                              trials=result["scripted"]["trials"])), flush=True)
        if not frozen.is_dir() or result["scripted"]["calibration"] != 20:
            result["models_skipped"] = "twenty-task synthetic history unavailable"
        else:
            for index, model in enumerate(PINS):
                result["models"][model] = model_run(base / f"model-{index+1}", binary, model, frozen)
                save(base / "readiness.json", result)
                print(json.dumps(dict(phase="model finished", model=model,
                    model_ready=result["models"][model]["model_ready"], trials=result["models"][model].get("trials",0))), flush=True)
    except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
        save(base / "private-error.json", dict(type=type(error).__name__, detail=str(error)))
        result["status"] = "incomplete; private diagnostics preserved"
    finally:
        after = coordinator.file_hash(provider) if provider.exists() else None
        result.update(provider_unchanged=before==after, ended_at=now())
        try:
            coordinator.check_pins(freeze, binary, time.monotonic()+10)
            result["frozen_implementation_unchanged"] = coordinator.file_hash(test_binary) == freeze["test_binary_sha256"]
        except (OSError, ValueError, TimeoutError):
            result["frozen_implementation_unchanged"] = False
        result["ready_for_registration_design"] = (result.get("scripted",{}).get("mechanics_ready",False)
            and len(result["models"])==2 and all(r["model_ready"] for r in result["models"].values())
            and before==after and result["frozen_implementation_unchanged"])
        readable = render_report(result)
        (base / "README.md").write_text(readable)
        if (base / "README.md").read_text() != readable:
            raise ValueError("readiness report readback mismatch")
        result["readable_report_readback"] = "verified locally"
        result["ended_at"] = now()
        save(base / "readiness.json", result)
    print(json.dumps(dict(ready_for_registration_design=result["ready_for_registration_design"],
                         s6a_complete=False, report=str(base/"readiness.json"))))
    return 0 if result["ready_for_registration_design"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
