#!/usr/bin/env python3
"""Bounded, private two-model S6a v2 execution; stdout contains aggregates only.

Run once after committing the implementation and building the release binary.
This never resumes, replaces a failed study, publishes, or activates a policy.
"""
import argparse
import contextlib
import datetime
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import time
import tomllib
import urllib.request

import s6a_evidence

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("s6a_runner", ROOT / "scripts/self-model-study.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
MODELS = ("qwen3.6:35b-a3b-q4_K_M", "llama3.1:8b")
PRIMARY_DIGEST = "07d35212591fc27746f0a317c975a6d68754fb38e9053d82e25f06057af28522"
LIMIT = 3_600_000
RESERVE = 300_000


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def file_hash(path, deadline=None):
    s6a_evidence.check_deadline(deadline)
    with Path(path).open("rb") as stream:
        hasher = hashlib.sha256()
        while chunk := stream.read(1024 * 1024):
            s6a_evidence.check_deadline(deadline)
            hasher.update(chunk)
    return hasher.hexdigest()


def save(path, value):
    runner.save(path, value)
    if json.loads(path.read_bytes()) != value:
        raise ValueError("private record readback failed")


class Clock:
    def __init__(self, path):
        self.path, self.phases = path, []
        self._active = {model: 0. for model in MODELS}
        self._models, self._last = MODELS, time.monotonic()
        save(path, dict(started_at=now(), active_ms=self.active, phases=[]))

    def charge(self):
        current = time.monotonic()
        for model in self._models:
            self._active[model] += current - self._last
        self._last = current

    @property
    def active(self):
        self.charge()
        return {model:math.ceil(seconds * 1000) for model, seconds in self._active.items()}

    @contextlib.contextmanager
    def phase(self, label, models):
        self.charge()
        previous, self._models = self._models, models
        start, captured = time.monotonic(), now()
        try:
            yield
        finally:
            elapsed = round((time.monotonic() - start) * 1000)
            self.charge()
            self.phases.append(dict(phase=label, models=list(models), started_at=captured,
                                    ended_at=now(), active_ms=elapsed))
            save(self.path, dict(active_ms=self.active, phases=self.phases,
                                 human_wait_ms=0, human_review="pending"))
            # Ledger serialization/readback is charged to this phase too. Gaps
            # between named phases are shared preparation, never free time.
            self.charge()
            self._models = previous

    def remaining(self, models, reserve=0):
        remaining = min(LIMIT - self.active[model] - reserve for model in models) / 1000
        if remaining <= 0:
            raise TimeoutError("registered active-time budget exhausted")
        return remaining


def process(argv, directory, timeout, env=None, input_bytes=None):
    """Capture setup diagnostics privately, with bounded process-tree lifetime."""
    if timeout <= 0:
        raise TimeoutError("no active time remains for child process")
    child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, cwd=directory, env=env, start_new_session=True)
    try:
        out, _ = child.communicate(input_bytes, timeout=timeout)
    except subprocess.TimeoutExpired:
        runner.stop_trial(child)
        child.communicate()
        raise TimeoutError("bounded child process expired") from None
    if child.returncode:
        raise RuntimeError("bounded child process failed")
    return out


def check_pins(freeze, binary, deadline):
    if file_hash(binary, deadline) != freeze["binary_sha256"]:
        raise ValueError("frozen executable changed")
    for path, digest in freeze["files"].items():
        if file_hash(ROOT / path, deadline) != digest:
            raise ValueError("frozen implementation or protocol changed")


def private_directory(base, real_data):
    base, real = Path(base).resolve(), Path(real_data).resolve()
    if (base.exists() or base.is_relative_to("/nix/store")
        or base.is_relative_to(real) or real.is_relative_to(base)
        or Path.home().resolve().is_relative_to(base)
        or any(part.startswith(".") for part in base.parts)
        or any((p / ".git").exists() for p in base.parents)):
        raise ValueError("study needs a new directory outside repositories, configuration and real memory")
    return base


def evaluator(directory, prompt, seconds, settings):
    """Fresh configured model, no user plugins/hooks/history, no hidden traces."""
    if seconds <= 0:
        raise TimeoutError("no active time remains for evaluator")
    deadline = time.monotonic() + seconds
    argv = ["codex", "exec", "--ephemeral", "--ignore-user-config", "--skip-git-repo-check",
            "--sandbox", "workspace-write", "-c", 'approval_policy="never"',
            "--disable", "hooks", "--disable", "apps", "--disable", "plugins",
            "--disable", "memories", "--disable", "multi_agent", "--disable", "shell_snapshot",
            "--enable", "skip_host_skill_discovery", "-c", 'web_search="disabled"',
            "--model", settings["model"], "-c", "model_reasoning_effort=" + json.dumps(settings["effort"]),
            "-C", str(directory), "--json", "-"]
    # Keep auth and the host's rule files. No HOME/CODEX_HOME substitution and
    # no bypass flags. stdout events are filtered before any disk persistence.
    child = subprocess.Popen(argv, cwd=directory, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, start_new_session=True)
    child.stdin.write(prompt.encode())
    child.stdin.close()
    selector = selectors.DefaultSelector()
    selector.register(child.stdout, selectors.EVENT_READ)
    metadata = dict(model=settings["model"], effort=settings["effort"], started_at=now(),
                    session=None, hooks=False, ephemeral=True, sandbox="workspace-write")
    buffer = b""
    try:
        with (directory / "visible-messages.jsonl").open("wb") as visible:
            while selector.get_map():
                if time.monotonic() >= deadline:
                    raise TimeoutError("independent evaluator budget exhausted")
                for key, _ in selector.select(min(1, deadline - time.monotonic())):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    buffer += chunk
                    if len(buffer) > 16 * 1024 * 1024:
                        raise ValueError("evaluator event exceeded bound")
                    while b"\n" in buffer:
                        line, buffer = buffer.split(b"\n", 1)
                        try:
                            event = json.loads(line)
                        except ValueError:
                            continue
                        if event.get("type") == "thread.started":
                            metadata["session"] = event["thread_id"]
                        item = event.get("item", {})
                        if event.get("type") == "item.completed" and item.get("type") == "agent_message":
                            visible.write(runner.encoded(dict(role="assistant", text=item.get("text", ""))) + b"\n")
                        # Reasoning and tool internals are deliberately not retained.
            child.wait(timeout=max(.001, deadline - time.monotonic()))
            if child.returncode or not metadata["session"]:
                raise RuntimeError("independent evaluator failed")
    finally:
        selector.close()
        if child.poll() is None:
            runner.stop_trial(child)
            child.wait()
        metadata.update(ended_at=now(), returncode=child.returncode)
        save(directory / "adapter.json", metadata)
    return metadata


def marker_present(path, marker, deadline):
    """Streaming byte search, avoiding loading the real memory index."""
    try:
        with path.open("rb") as stream:
            tail = b""
            while chunk := stream.read(1024 * 1024):
                if time.monotonic() >= deadline:
                    raise TimeoutError("isolation scan exceeded preparation budget")
                data = tail + chunk
                if marker in data:
                    return True
                tail = data[-len(marker):]
    except (OSError, ValueError):
        raise ValueError("could not establish evaluator isolation") from None
    return False


def check_isolation(base, settings, real_data, seconds):
    deadline = time.monotonic() + seconds
    directory = base / "isolation"
    directory.mkdir(mode=0o700)
    marker = ("s6a-disposable-" + os.urandom(24).hex()).encode()
    (directory / "marker.txt").write_bytes(marker)
    prompt = ("You are a disposable isolation probe. Read only marker.txt in this directory. "
              "Write its exact contents to observed.txt, then repeat its contents in your final reply. "
              "Do not use memory, apps, network tools, skills, other directories, or other agents.")
    metadata = evaluator(directory, prompt, seconds, settings)
    if (directory / "observed.txt").read_bytes() != marker:
        raise ValueError("isolation probe did not execute")
    # A deliberately visible marker must not have entered normal Lucy memory or
    # persisted Codex rollout directories. Read only for this equality check.
    roots = [Path(real_data), Path.home() / ".codex/sessions", Path.home() / ".codex/archived_sessions"]
    for root in roots:
        if root.exists():
            for path in root.rglob("*"):
                if path.is_file() and not path.is_symlink() and marker_present(path, marker, deadline):
                    raise ValueError("evaluator marker entered ordinary memory or rollout history")
    return dict(passed=True, adapter=metadata, marker_sha256=hashlib.sha256(marker).hexdigest(),
                normal_memory_marker_found=False)


def identity(binary, directory, env, seconds):
    return json.loads(process([str(binary), "self-model", "task", "study-info"], directory, seconds, env))


def model_setup(binary, base, name, seconds):
    start = time.monotonic()
    with urllib.request.urlopen("http://127.0.0.1:11434/api/tags", timeout=min(15, seconds)) as response:
        catalog = json.load(response)["models"]
    if not any(row["name"] == name for row in catalog):
        if name == MODELS[0]:
            raise ValueError("registered primary model is unavailable")
        env = dict(os.environ, OLLAMA_HOST="127.0.0.1:11434")
        process(["ollama", "pull", name], base, seconds - (time.monotonic() - start), env)
        with urllib.request.urlopen("http://127.0.0.1:11434/api/tags", timeout=15) as response:
            catalog = json.load(response)["models"]
    digest = next(row["digest"].removeprefix("sha256:") for row in catalog if row["name"] == name)
    if len(digest) != 64 or (name == MODELS[0] and digest != PRIMARY_DIGEST):
        raise ValueError("registered model pin changed")
    env = dict(os.environ, LUCY_CHAT_MODEL=name, LUCY_CHAT_DIGEST=digest)
    info = identity(binary, base, env, seconds - (time.monotonic() - start))
    if info["model"] != f"ollama:{name}@{digest}":
        raise ValueError("provider differs from the registered local model")
    return env, info


def seal_cases(base, freeze, settings, seconds):
    directory = base / "selection"
    directory.mkdir(mode=0o700)
    for source in [ROOT / "roadmap/S6a.v2.md", ROOT / "scripts/S6A_EVALUATOR.md"]:
        shutil.copyfile(source, directory / source.name)
    save(directory / "freeze.json", freeze)
    prompt = ("You are the independent S6a case selector in a fresh session with no builder history. "
              "Read S6A_EVALUATOR.md, S6a.v2.md and freeze.json here and carry out only the case-selection instructions. "
              "Do not run any evaluated model or inspect any external memory, repository, or conversation. "
              "Save your own generation/validation code here, the bundle as bundle.json, and a separate rationale.md. "
              "Use the actual CODEX_THREAD_ID supplied to your shell as bundle.evaluator. "
              "Finish with counts and the file's SHA-256 only; no cases or answers in the final reply.")
    metadata = evaluator(directory, prompt, seconds, settings)
    bundle = json.loads((directory / "bundle.json").read_bytes())
    runner.validate_bundle(bundle)
    if bundle["evaluator"] != metadata["session"] or bundle["evaluator_model"] != settings["model"]:
        raise ValueError("case selector attribution does not match the actual adapter")
    if bundle["implementation"] != freeze["implementation"] or bundle["protocol"] != freeze["protocol"]:
        raise ValueError("case selection pins changed")
    save(base / "sealed-bundle.json", bundle)
    return dict(adapter=metadata, bundle_sha256=file_hash(base / "sealed-bundle.json"),
                bundle_digest=runner.digest(bundle), calibration=len(bundle["calibration"]), held_out=len(bundle["held_out"]))


def independent_review(base, freeze, settings, seconds):
    directory = base / "review"
    directory.mkdir(mode=0o700)
    shutil.copyfile(ROOT / "scripts/S6A_EVALUATOR.md", directory / "S6A_EVALUATOR.md")
    save(directory / "freeze.json", freeze)
    # A separate fresh session can inspect the explicitly supplied study root,
    # never builder history. It must not edit any source material or retry trials.
    prompt = ("You are the independent S6a adjudicator in a fresh session. Read S6A_EVALUATOR.md "
              "for the review instructions (do NOT generate cases), and freeze.json. "
              f"Read the preserved evidence under {base} only. Do not change originals or run evaluated models. "
              "Inspect selection provenance, the sealed ground truth, Python replay summaries, Rust reports, "
              "a sample of actual input/output originals and visible exchanges for each available model/family/arm. "
              "Write review.json and review.md here. If evidence is incomplete, call it incomplete. "
              "The review.json object must contain reviewer (actual CODEX_THREAD_ID), model, "
              "accept_evidence (boolean), technical_qualification (boolean), disagreements (array of strings), "
              "limitations (array of strings), and s6a_complete:false. Final response: aggregate status only. "
              "Never infer human approval. Remain within the time available; prioritize missing evidence and invalid results.")
    metadata = evaluator(directory, prompt, seconds, settings)
    review = json.loads((directory / "review.json").read_bytes())
    if review.get("reviewer") != metadata["session"] or review.get("model") != settings["model"]:
        raise ValueError("review attribution does not match adapter")
    validate_review(review)
    return dict(adapter=metadata, review=review, report_sha256=file_hash(directory / "review.json"))


def validate_review(review):
    if (review.get("s6a_complete") is not False
        or any(type(review.get(key)) is not bool for key in ("accept_evidence", "technical_qualification"))
        or any(not isinstance(review.get(key), list) or any(not isinstance(s, str) for s in review[key])
               for key in ("disagreements", "limitations"))
        or (review["technical_qualification"] and (not review["accept_evidence"] or review["disagreements"]))):
        raise ValueError("invalid or contradictory independent review")


def technical_qualification(reports, states, review, active_ms):
    """Shared final gate, also exercised with explicitly synthetic evidence."""
    validate_review(review)
    return (len(reports) == 2 and len(states) == 2
        and all(report["complete"] and report.get("live") is True and len(report["scopes"]) == 2
                and all(scope["eligible"] and scope["forecast_qualifies"] for scope in report["scopes"])
                for report in reports)
        and all(state.get("replay", {}).get("complete_originals", False)
                and state.get("score_replay", {}).get("technical_qualification", False) for state in states)
        and review["accept_evidence"] and review["technical_qualification"] and not review["disagreements"]
        and len(active_ms) == 2 and all(value <= LIMIT for value in active_ms))


def qualification_status(qualified):
    return dict(technical_qualification=qualified, s6a_complete=False,
                status="awaiting human adjudication" if qualified else "incomplete or did not qualify")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    binary, base = args.binary.resolve(strict=True), args.directory.resolve()
    real_data = os.environ.get("LUCY_DATA_DIR", str(Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "lucy"))
    try:
        base = private_directory(base, real_data)
    except ValueError as error:
        parser.error(str(error))
    # Implementation preparation precedes the registered study clock.
    status = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, check=True).stdout
    if status:
        parser.error("commit and freeze implementation before selecting cases")
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, check=True).stdout.decode().strip()
    config = tomllib.loads((Path.home() / ".codex/config.toml").read_text())
    if config.get("model_provider", "openai") != "openai":
        parser.error("isolated evaluator must preserve the configured provider; unsupported provider")
    settings = dict(model=config["model"], effort=config.get("model_reasoning_effort", "medium"))
    base.mkdir(parents=True, mode=0o700)
    clock = Clock(base / "clock.json")
    states, envs, infos = {}, {}, {}
    freeze = dict(schema=2, source_commit=commit, binary_sha256=file_hash(binary), evaluator=settings,
                  files={str(p.relative_to(ROOT)): file_hash(p) for p in [
                      ROOT / "scripts/self-model-study.py", ROOT / "scripts/s6a_evidence.py",
                      Path(__file__), ROOT / "scripts/S6A_EVALUATOR.md", ROOT / "roadmap/S6a.v2.md"]})
    provider_path = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "lucy/providers.json"
    provider_before = file_hash(provider_path) if provider_path.exists() else None
    outcome = dict(schema=2, started_at=now(), s6a_complete=False, technical_qualification=False,
                   adaptation_enabled=False, human_review="pending", publication="none", status="incomplete")
    save(base / "status.json", outcome)
    try:
        for name in MODELS:
            with clock.phase("model setup", [name]):
                envs[name], infos[name] = model_setup(binary, base, name, clock.remaining([name], RESERVE))
        freeze.update({key:infos[MODELS[0]][key] for key in ("implementation", "protocol", "checker", "policy")})
        freeze["models"] = {name:infos[name]["model"] for name in MODELS}
        if freeze["implementation"] != freeze["binary_sha256"] or any(
            infos[name][key] != freeze[key] for name in MODELS for key in ("implementation", "protocol", "checker", "policy")):
            raise ValueError("frozen native identities disagree")
        save(base / "freeze.json", freeze)
        with clock.phase("evaluator isolation", MODELS):
            isolation = check_isolation(base, settings, infos[MODELS[0]]["real_data"], clock.remaining(MODELS, RESERVE))
            save(base / "isolation.json", isolation)
        with clock.phase("independent case selection", MODELS):
            selection = seal_cases(base, freeze, settings, clock.remaining(MODELS, RESERVE))
            save(base / "selection.json", selection)
        clock.remaining(MODELS, RESERVE)
        for index, name in enumerate(MODELS):
            with clock.phase("native trials and artifact scoring", [name]):
                check_pins(freeze, binary, time.monotonic() + clock.remaining([name], RESERVE))
                current = identity(binary, base, envs[name], clock.remaining([name], RESERVE))
                if current["model"] != freeze["models"][name]:
                    raise ValueError("frozen model tag changed")
                if file_hash(base / "sealed-bundle.json") != selection["bundle_sha256"]:
                    raise ValueError("sealed bundle changed")
                prior = clock.active[name]
                run_dir = base / f"model-{index + 1}"
                argv = ["python3", str(ROOT / "scripts/self-model-study.py"), "--binary", str(binary),
                        "--bundle", str(base / "sealed-bundle.json"), "--directory", str(run_dir),
                        "--prior-active-ms", str(prior), "--reserve-seconds", "300"]
                # The runner's nonzero code is an expected incomplete outcome.
                child = subprocess.Popen(argv, cwd=ROOT, env=envs[name], stdout=subprocess.DEVNULL,
                                         stderr=subprocess.DEVNULL, start_new_session=True)
                try:
                    child.wait(timeout=clock.remaining([name]))
                except subprocess.TimeoutExpired:
                    runner.stop_trial(child)
                    child.wait()
                states[name] = dict(directory=str(run_dir), returncode=child.returncode)
            save(base / "status.json", dict(outcome, phase="model finished", models=states, active_ms=clock.active))
            print(json.dumps(dict(phase="model finished", model=name, active_ms=clock.active[name])), flush=True)
        with clock.phase("independent replay and review", MODELS):
            review_deadline = time.monotonic() + clock.remaining(MODELS) - 5
            for name in MODELS:
                try:
                    verified = {}
                    states[name]["replay"] = s6a_evidence.verify_run(states[name]["directory"], review_deadline, verified)
                    states[name]["score_replay"] = s6a_evidence.verify_score(states[name]["directory"], review_deadline, verified)
                except (OSError, ValueError, KeyError, TypeError):
                    states[name]["replay"] = dict(valid=False)
            save(base / "replay.json", states)
            # Five seconds remain for final metadata and readback.
            review = independent_review(base, freeze, settings, review_deadline - time.monotonic())
            save(base / "independent-review.json", review)
        reports = [json.loads((Path(states[name]["directory"]) / "report.json").read_bytes()) for name in MODELS]
        check_pins(freeze, binary, time.monotonic() + clock.remaining(MODELS))
        for name, report in zip(MODELS, reports):
            if report.get("model") != freeze["models"][name] or any(report.get(k) != freeze[k] for k in ("protocol", "implementation")):
                raise ValueError("scored model differs from frozen model")
        qualified = technical_qualification(reports, [states[name] for name in MODELS], review["review"], list(clock.active.values()))
        outcome.update(qualification_status(qualified))
    except (OSError, RuntimeError, ValueError, TimeoutError, KeyError, TypeError) as error:
        # Keep detailed errors private. No automatic retries or replacement cases.
        save(base / "private-error.json", dict(type=type(error).__name__, detail=str(error)))
        outcome["status"] = "incomplete; private diagnostics preserved"
    finally:
        with clock.phase("final metadata and readback", MODELS):
            provider_after = file_hash(provider_path) if provider_path.exists() else None
            outcome.update(models=states, source_commit=commit,
                           provider_unchanged=provider_before == provider_after,
                           persistence="local originals; no replication attempted")
            if provider_before != provider_after:
                outcome["technical_qualification"] = False
                outcome["status"] = "invalid: provider configuration changed"
            try:
                deadline = time.monotonic() + clock.remaining(MODELS) - 1
                originals = {}
                for path in base.rglob("*"):
                    s6a_evidence.check_deadline(deadline)
                    if path.is_symlink():
                        raise ValueError("symlink in preserved study")
                    if path.is_file() and (path.suffix in (".json", ".jsonl", ".md", ".py", ".txt", ".stderr", ".stdout", ".partial")
                        or path.relative_to(base).parts[0] in ("selection", "review", "isolation")):
                        if path.name not in ("clock.json", "status.json", "final-report.json", "preserved-hashes.json"):
                            originals[str(path.relative_to(base))] = file_hash(path, deadline)
                save(base / "preserved-hashes.json", originals)
                if any(file_hash(base / name, deadline) != sha for name, sha in originals.items()):
                    raise ValueError("preserved original readback mismatch")
                outcome["original_readback"] = "verified"
            except (OSError, TimeoutError, ValueError):
                outcome["original_readback"] = "incomplete"
                outcome["technical_qualification"] = False
                outcome["status"] = "incomplete: original readback unavailable"
        if any(value > LIMIT for value in clock.active.values()):
            outcome["technical_qualification"] = False
            outcome["status"] = "incomplete: active-time ceiling exceeded"
        outcome["active_ms"] = clock.active
        outcome["ended_at"] = now()
        save(base / "final-report.json", outcome)
        print(json.dumps(dict(status=outcome["status"], technical_qualification=outcome["technical_qualification"],
                              s6a_complete=False, active_ms=clock.active, report=str(base / "final-report.json"))), flush=True)
    return 0 if outcome["technical_qualification"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
