"""Independent, non-inference replay of private S6a artifacts.

This module does not import the Rust oracle or accept a model's completion prose.
Its CLI prints aggregate validity only, never held-out content.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import copy
import time


def check_deadline(deadline):
    if deadline is not None and time.monotonic() >= deadline:
        raise TimeoutError("evidence replay reached the registered deadline")


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def manifest(directory, deadline=None):
    directory = Path(directory)
    result, size = {}, 0
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("artifact directory unavailable or symlinked")
    for path in sorted(directory.rglob("*")):
        check_deadline(deadline)
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("unsafe artifact type")
        if path.is_file():
            size += path.stat().st_size
            if size > 64 * 1024 * 1024 or len(result) >= 1024:
                raise ValueError("artifact bounds exceeded")
            result[str(path.relative_to(directory))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def snapshot(source, target):
    manifest(source)
    shutil.copytree(source, target)
    if manifest(source) != manifest(target):
        raise ValueError("partial artifact snapshot changed during capture")


def validate_oracle(case):
    outputs = {rule["path"] for rule in case["output_writes"]}
    produced, attributed = False, case["definition"]["family"] != "retrieval"
    for row in case["oracle"]:
        check = row["check"]
        if check["kind"] not in ("file_exists", "file_sha256", "json_equals", "record"):
            raise ValueError("oracle must use independent replay primitives")
        if check["kind"] in ("file_sha256", "json_equals") and check["path"] in outputs:
            produced |= check.get("pointer") != "/source"
            attributed |= check.get("pointer") == "/source"
    if not produced or not attributed:
        raise ValueError("oracle must inspect produced output and retrieval attribution")


def concealed(value):
    """Native expectations stay sealed even when task status is requested."""
    if isinstance(value, dict):
        if isinstance(value.get("check"), dict) and {"expected", "sha256"} & value["check"].keys():
            return False
        return all(concealed(v) for v in value.values())
    if isinstance(value, list):
        return all(concealed(v) for v in value)
    return True


def pointer(value, path):
    for part in path.split("/")[1:] if path else []:
        part = part.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            if not part.isdecimal() or (len(part) > 1 and part.startswith("0")):
                raise KeyError(part)
            value = value[int(part)]
        else:
            value = value[part]
    return value


def json_equal(left, right):
    # serde_json distinguishes booleans, integer numbers and floating numbers;
    # float signed zero compares equal. Python's True == 1 must not count.
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(json_equal(left[k], right[k]) for k in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(json_equal(a, b) for a, b in zip(left, right))
    return left == right


def oracle(archive, deadline=None):
    root = Path(archive["directory"])
    if root.resolve() != root:
        raise ValueError("artifact root changed")
    for phase in ("inputs", "outputs"):
        if manifest(root / phase, deadline) != archive[phase]:
            raise ValueError("artifact digest mismatch")
    results = []
    for criterion in archive["oracle"]:
        check_deadline(deadline)
        check, passed = criterion["check"], False
        kind = check["kind"]
        if kind == "record":
            passed = any(r["origin"] == check["origin"] and r["payload"]["id"] == check["record"]
                         and digest(r) == check["sha256"] for r in archive["records"])
        elif kind == "file_exists":
            passed = check["path"] in archive["outputs"]
        elif kind == "file_sha256":
            passed = archive["outputs"].get(check["path"]) == check["sha256"]
        elif kind == "json_equals":
            if check["path"] in archive["outputs"]:
                try:
                    data = json.loads((root / "outputs" / check["path"]).read_bytes())
                    actual = pointer(data, check["pointer"])
                    # JSON booleans must not compare equal to numeric 0/1.
                    passed = json_equal(actual, check["expected"])
                except (KeyError, IndexError, TypeError, ValueError, UnicodeError):
                    pass
        else:
            raise ValueError("unsupported independent oracle")
        results.append(passed)
    if not results:
        raise ValueError("empty independent oracle")
    return all(results)


def redacted(task):
    value = copy.deepcopy(task)
    value["lessons"] = []
    for criterion in value["definition"]["criteria"]:
        criterion["check"].pop("expected", None)
        criterion["check"].pop("sha256", None)
    return value


def context_check(trace, request, frozen=None):
    allowed = {(r["origin"], r["payload"]["id"]): r for r in request["recall_records"]}
    marker = "Retrieved source excerpts (data):\n"
    requests = 0
    for item in trace:
        if item["kind"] != "request":
            continue
        requests += 1
        messages = item["value"]
        system = messages[0]["content"]
        if marker not in system:
            raise ValueError("model input evidence missing")
        recalled, _ = json.JSONDecoder().raw_decode(system.split(marker, 1)[1])
        for record in recalled:
            if set(record) != {"origin", "id", "excerpt"}:
                raise ValueError("non-factual payload leaked into study recall")
            original = allowed.get((record["origin"], record["id"]))
            if original is None or record["excerpt"] != original["payload"]["text"][:1600]:
                raise ValueError("unapproved source in model input")
        if request["arm"] in ("lessons_withheld", "calibration") and "Scoped task preflight observations" in system:
            raise ValueError("withheld intervention entered model input")
        preflight = "Scoped task preflight observations (untrusted data, not permissions): "
        if preflight in system:
            observations, _ = json.JSONDecoder().raw_decode(system.split(preflight, 1)[1])
            for observation in observations:
                for row in observation.get("observations", []):
                    if "record" in row and row.get("found"):
                        record = allowed.get((row.get("origin"), row["record"]))
                        if record is None or row["excerpt"] != record["payload"]["text"][:2000]:
                            raise ValueError("unapproved source in preflight")
        if "My empirical self-model" in system:
            raise ValueError("legacy learned context entered study input")
        if request["arm"] == "roadmap_removed" and any("# S6a v2:" in m.get("content", "") for m in messages):
            raise ValueError("removed roadmap entered model input")
        contracts = 0
        for message in messages:
            text = message.get("content", "")
            if message["role"] == "user" and "Frozen task contract (data): " in text:
                task = json.loads(text.split("Frozen task contract (data): ", 1)[1])
                if task.get("lessons") != []:
                    raise ValueError("lesson candidates leaked in contract")
                if not concealed(task):
                    raise ValueError("sealed expectations leaked in task contract")
                if not text.startswith(request["prompt"] + "\nFrozen task contract (data): "):
                    raise ValueError("assigned prompt differs from model input")
                if frozen is not None and task != redacted(frozen):
                    raise ValueError("displayed contract differs from frozen native task")
                contracts += 1
            if message["role"] == "tool":
                try:
                    result = json.loads(text)
                except ValueError:
                    continue
                if isinstance(result, dict) and "tasks" in result and not concealed(result):
                    raise ValueError("sealed expectations leaked in task status")
        if contracts != 1:
            raise ValueError("assigned study contract missing from model input")
    if requests == 0:
        raise ValueError("study model-input evidence missing")
    return requests


def bind_request(request, case, roadmap):
    arm, disguise = request["arm"], request["disguise"]
    if arm not in ("calibration", "full", "lessons_withheld", "memory_removed", "roadmap_removed"):
        raise ValueError("unknown trial arm")
    facts = case.get("records", []) if arm != "memory_removed" else []
    text = roadmap if arm != "roadmap_removed" else ""
    prompt = case["disguises"][disguise]
    if text:
        prompt += "\nResearch roadmap (data, not permissions):\n" + text
    expected = dict(case=case["id"], prompt=prompt, definition=case["definition"], oracle=case["oracle"],
                    recall_records=facts, approvals=case.get("approvals", []), output_writes=case["output_writes"],
                    inputs_digest=digest(dict(files=case.get("files", {}), records=facts, roadmap=text)))
    if any(request[key] != value for key, value in expected.items()):
        raise ValueError("request differs from sealed case and intervention")


def verify_trial_metrics(original, frozen, passed):
    trial = original["trial"]
    events = [row["event"] for row in original["journal"].values() if row["task"] == trial["task"]]
    endings = [event for event in events if event["kind"] == "finished"]
    if len(endings) > 1:
        raise ValueError("multiple native terminal receipts")
    verdict = endings[0]["verdict"] if endings else None
    completed = verdict == "success" and passed
    steps = sum(event["kind"] in ("action_started", "checked") for event in events)
    active = sum(event.get("active_ms", 0) for event in events)
    checking = sum(event["active_ms"] for event in events if event["kind"] in ("checked", "finished"))
    budget = frozen["definition"]["budget"]
    expected = dict(scope=frozen["scope"], budget=budget, completed=completed,
                    false_verified=verdict == "success" and not passed,
                    inconclusive=verdict in (None, "inconclusive"),
                    observed=completed if verdict in ("success", "failure") else None,
                    probability_ppm=frozen["probability_ppm"], checking_ms=checking,
                    budget_violation=steps > budget["steps"] or active > budget["active_ms"]
                        or original["native_active_ms"] > budget["active_ms"])
    if any(not json_equal(trial[key], value) for key, value in expected.items()):
        raise ValueError("trial metrics disagree with native receipts and independent oracle")
    if not active <= original["native_active_ms"] <= trial["active_ms"]:
        raise ValueError("trial durations disagree with native receipts")


def verify_original(path, request, case, roadmap, deadline=None):
    check_deadline(deadline)
    bind_request(request, case, roadmap)
    original = json.loads(Path(path).read_bytes())
    if original.get("schema") != 2:
        raise ValueError("legacy original is not replayable v2 evidence")
    archive = original["artifacts"]
    if archive["directory"] != request["archive"] or archive["oracle"] != case["oracle"]:
        raise ValueError("original not bound to sealed oracle and archive")
    expected = {name: hashlib.sha256(text.encode()).hexdigest() for name, text in case.get("files", {}).items()}
    if archive["inputs"] != expected:
        raise ValueError("trial inputs differ from sealed fixtures")
    tasks = [entry["event"]["task"] for entry in original["journal"].values()
             if entry["event"]["kind"] == "begin" and entry["task"] == original["trial"]["task"]]
    if len(tasks) != 1:
        raise ValueError("missing frozen native task")
    definition = tasks[0]["definition"]
    for key in ("title", "family", "method", "criteria", "budget"):
        if definition[key] != case["definition"][key]:
            raise ValueError("native contract differs from sealed case")
    for key in ("case", "disguise", "arm", "inputs_digest"):
        if original["trial"][key] != request[key]:
            raise ValueError("original trial identity differs from request")
    passed = oracle(archive, deadline)
    native = original["oracle"]["event"].get("verdict") == "success"
    if native != passed:
        raise ValueError("independent oracle disagrees with native receipt")
    verify_trial_metrics(original, tasks[0], passed)
    context_check(original["visible_trace"], request, tasks[0])
    return {"valid": True, "oracle_pass": passed, "trial": original["trial"]}


def verify_run(directory, deadline=None, verified=None):
    base = Path(directory)
    bundle = json.loads((base / "sealed-bundle.json").read_bytes())
    roadmap = (base / "protocol.md").read_text()
    if hashlib.sha256(roadmap.encode()).hexdigest() != bundle["protocol"]:
        raise ValueError("archived protocol changed")
    cases = {case["id"]: case for phase in ("calibration", "held_out") for case in bundle[phase]}
    verified = {} if verified is None else verified
    expected = {(case["id"], "a", "calibration") for case in bundle["calibration"]}
    expected |= {(case["id"], disguise, arm) for case in bundle["held_out"] for disguise in ("a", "b")
                 for arm in ("full", "lessons_withheld", "memory_removed", "roadmap_removed")}
    seen, tasks, completed = set(), set(), set()
    for request_path in sorted(base.glob("*-request.json")):
        check_deadline(deadline)
        request = json.loads(request_path.read_bytes())
        identity = (request["case"], request["disguise"], request["arm"])
        if identity not in expected or identity in seen:
            raise ValueError("unplanned or duplicated trial request")
        seen.add(identity)
        label = request_path.name.removesuffix("-request.json")
        original = base / (label + ".json") if label.startswith("calibration-") else base / label / "trial.json"
        if original.is_file():
            result = verify_original(original, request, cases[request["case"]], roadmap, deadline)
            if original.resolve() != original or str(original) in verified or result["trial"]["task"] in tasks:
                raise ValueError("reused original or native task identity")
            tasks.add(result["trial"]["task"])
            completed.add(identity)
            verified[str(original)] = hashlib.sha256(original.read_bytes()).hexdigest()
    return {"valid": True, "originals_verified": len(verified), "complete_originals": completed == expected}


def interval(differences, deadline=None):
    seed, samples, mask = 6, [], (1 << 64) - 1
    for _ in range(10000):
        check_deadline(deadline)
        total = 0
        for _ in differences:
            seed ^= (seed << 13) & mask
            seed ^= seed >> 7
            seed ^= (seed << 17) & mask
            total += differences[seed % len(differences)]
        samples.append(total / len(differences))
    samples.sort()
    def quantile(p):
        position = (len(samples) - 1) * p
        low = int(position)
        return samples[low] + (samples[min(low + 1, len(samples) - 1)] - samples[low]) * (position - low)
    return [quantile(.025), quantile(.975)]


def verify_score(directory, deadline=None, verified=None):
    """Recompute statistical summaries without the Rust scorer."""
    base = Path(directory)
    if verified is None:
        verified = {}
        verify_run(base, deadline, verified)
    scoring = json.loads((base / "score-input.json").read_bytes())
    study = scoring["study"]
    report = json.loads((base / "report.json").read_bytes())
    info = json.loads((base / "identity.json").read_bytes())
    bundle = json.loads((base / "sealed-bundle.json").read_bytes())
    if study["bundle_digest"] != digest(bundle) or study["schema"] != 2:
        raise ValueError("scorer sealed bundle mismatch")
    for key in ("model", "protocol", "implementation"):
        if study[key] != info[key] or report.get(key) != study[key]:
            raise ValueError("scorer frozen identity mismatch")
    def scope_key(trial):
        return digest(dict(scope=trial["scope"], checker=info["checker"], policy=info["policy"]))
    def original_trial(path):
        check_deadline(deadline)
        path = Path(path)
        if path.resolve() != path or not path.is_relative_to(base):
            raise ValueError("scorer original outside study")
        raw = path.read_bytes()
        if verified.get(str(path)) != hashlib.sha256(raw).hexdigest():
            raise ValueError("scorer original was not independently replayed")
        return json.loads(raw)["trial"]
    if len(scoring["originals"]) != len(study["trials"]):
        raise ValueError("scorer original population mismatch")
    if any(original_trial(path) != trial for path, trial in zip(scoring["originals"], study["trials"])):
        raise ValueError("scorer metrics differ from saved original")
    calibration = [original_trial(path) for path in scoring["calibration_originals"]]
    counts = {}
    for trial in calibration:
        key = scope_key(trial)
        counts[key] = counts.get(key, 0) + 1
        if trial["arm"] != "calibration" or trial["model"] != info["model"]:
            raise ValueError("scorer calibration identity mismatch")
        if trial["case"] not in {case["id"] for case in bundle["calibration"]} or trial["disguise"] != "a":
            raise ValueError("scorer calibration differs from planned cases")
    if counts != study["calibration"] or len({t["case"] for t in calibration}) != len(calibration):
        raise ValueError("scorer calibration population mismatch")
    calibration_safe = all(not t["false_verified"] and not t["budget_violation"] for t in calibration)
    groups = {}
    identities = set()
    planned = {case["id"] for case in bundle["held_out"]}
    for row in study["trials"]:
        key = scope_key(row)
        identity = (key, row["case"], row["disguise"], row["arm"])
        if (identity in identities or row["case"] not in planned or row["model"] != info["model"]
            or row["scope"]["model"] != info["model"] or not 0 <= row["probability_ppm"] <= 1000000):
            raise ValueError("scorer held-out identity mismatch")
        identities.add(identity)
        groups.setdefault(key, []).append(row)
    if len(report["scopes"]) != len(groups) or {s["scope"] for s in report["scopes"]} != groups.keys():
        raise ValueError("scorer scope population mismatch")
    if report.get("complete") and not study["complete"]:
        raise ValueError("incomplete study relabelled complete")
    all_safe = calibration_safe and all(not t["false_verified"] and not t["budget_violation"] for t in study["trials"])
    base_eligible = (study["live"] and study["complete"] and report["complete"]
        and report["active_ms"] <= 3600000 and "revision-unknown" not in study["model"]
        and len(groups) == len(counts) == 2 and all(counts.get(k) == 10 for k in groups)
        and {row["scope"]["family"] for row in study["trials"]} == {"retrieval", "verification"})
    valid_scopes = {}
    for key, rows in groups.items():
        matrix = {(t["case"], t["disguise"], t["arm"]) for t in rows}
        cases = {t["case"] for t in rows}
        valid = len(cases) == 20 and counts.get(key) == 10 and len(rows) == 160
        valid &= matrix == {(case, disguise, arm) for case in cases for disguise in ("a", "b")
                           for arm in ("full", "lessons_withheld", "memory_removed", "roadmap_removed")}
        for case in cases:
            for disguise in ("a", "b"):
                selected = [t for t in rows if t["case"] == case and t["disguise"] == disguise]
                if selected:
                    valid &= all(t["criteria_digest"] == selected[0]["criteria_digest"] and t["budget"] == selected[0]["budget"] for t in selected)
                paired = [t for t in selected if t["arm"] in ("full", "lessons_withheld")]
                valid &= len(paired) == 2 and paired[0]["inputs_digest"] == paired[1]["inputs_digest"]
        valid_scopes[key] = bool(valid)
    qualified = []
    for scope in report["scopes"]:
        check_deadline(deadline)
        rows = groups[scope["scope"]]
        if scope["family"] != rows[0]["scope"]["family"] or scope["complete"] != valid_scopes[scope["scope"]]:
            raise ValueError("scorer scope completeness mismatch")
        if len(scope["arms"]) != 4 or {arm["arm"] for arm in scope["arms"]} != {"full", "lessons_withheld", "memory_removed", "roadmap_removed"}:
            raise ValueError("scorer arm summaries missing or duplicated")
        for arm in scope["arms"]:
            if len(arm["by_disguise"]) != 2 or {s["disguise"] for s in arm["by_disguise"]} != {"a", "b"}:
                raise ValueError("scorer presentation summaries missing or duplicated")
            metric_groups = [(arm, [row for row in rows if row["arm"] == arm["arm"]])]
            metric_groups += [(sub, [row for row in metric_groups[0][1] if row["disguise"] == sub["disguise"]])
                       for sub in arm["by_disguise"]]
            for summary, selected in metric_groups:
                for field, key in (("completed", "completed"), ("false_verified", "false_verified"),
                                   ("inconclusive", "inconclusive"), ("budget_violations", "budget_violation")):
                    if summary[field] != sum(row[key] for row in selected):
                        raise ValueError("scorer count mismatch")
                scores = [(row["probability_ppm"] / 1000000 - int(row["observed"])) ** 2
                          for row in selected if row["observed"] is not None]
                if summary["trials"] != len(selected) or summary["scored"] != len(scores):
                    raise ValueError("scorer population mismatch")
                brier = sum(scores) / len(scores) if scores else None
                if (brier is None) != (summary["brier"] is None) or (brier is not None and abs(brier - summary["brier"]) > 1e-12):
                    raise ValueError("scorer Brier mismatch")
                for field in ("active_ms", "checking_ms"):
                    if summary[field] != sum(row[field] for row in selected):
                        raise ValueError("scorer duration mismatch")
        if scope["complete"]:
            differences = []
            for case in sorted({row["case"] for row in rows}):
                paired = 0
                for disguise in ("a", "b"):
                    full, cold = [[row for row in rows if row["case"] == case and row["disguise"] == disguise
                                   and row["arm"] == arm] for arm in ("full", "lessons_withheld")]
                    if len(full) != 1 or len(cold) != 1:
                        raise ValueError("paired comparison lacks original trials")
                    paired += (int(full[0]["completed"]) - int(cold[0]["completed"])) / 2
                differences.append(paired)
            ci = interval(differences, deadline)
            if len(differences) != 20 or abs(sum(differences) / 20 - scope["completion_delta"]) > 1e-12:
                raise ValueError("scorer completion difference mismatch")
            if any(abs(a - b) > 1e-12 for a, b in zip(ci, scope["paired_ci95"])):
                raise ValueError("scorer paired interval mismatch")
        elif scope["completion_delta"] is not None or scope["paired_ci95"] is not None:
            raise ValueError("missing comparisons must remain unknown")
        full = next(arm for arm in scope["arms"] if arm["arm"] == "full")
        forecast = full["scored"] > 0 and full["brier"] < .25
        eligible = bool(base_eligible and all_safe and all(valid_scopes.values()) and scope["complete"]
                        and scope["completion_delta"] >= .10 and scope["paired_ci95"][0] > 0)
        if scope["eligible"] != eligible or scope["forecast_qualifies"] != forecast:
            raise ValueError("scorer qualification gate mismatch")
        qualified.append(eligible and forecast)
    return dict(valid=True, scopes=len(report["scopes"]), complete=report["complete"],
                technical_qualification=len(qualified) == 2 and all(qualified), calibration_safe=calibration_safe)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        result = verify_run(args.directory)
    except (OSError, ValueError, KeyError, TypeError):
        result = {"valid": False, "reason": "independent replay failed; inspect in isolated review only"}
    print(json.dumps(result))
    raise SystemExit(0 if result["valid"] else 2)
