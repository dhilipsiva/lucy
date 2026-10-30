"""Additional engineering checks; older S6a originals keep their replay contract."""
import json
from pathlib import Path

import s6a_evidence as evidence


FIELDS = {
    "list": ({"path": {"type": "string"}}, []),
    "read": ({"path": {"type": "string"}}, ["path"]),
    "search": ({"query": {"type": "string"}}, ["query"]),
    "write": ({"path": {"type": "string"}, "text": {"type": "string"}}, ["path", "text"]),
    "command": ({"argv": {"type": "array", "items": {"type": "string"}}}, ["argv"]),
    "task_check": ({"criterion": {"type": "string"}}, ["criterion"]),
    "task_finish": ({}, []),
    "task_status": ({}, []),
}
OBSERVATIONS = ("contract_change_calls", "unavailable_tool_calls", "invalid_argument_calls",
                "multiple_tool_replies", "premature_prose_endings", "writes_missing_required_checks")


def counts():
    return dict.fromkeys(OBSERVATIONS, 0)


def valid_arguments(name, raw):
    if isinstance(raw, str):
        try:
            raw = json.loads(raw)
        except ValueError:
            return False
    if not isinstance(raw, dict):
        return False
    properties, required = FIELDS[name]
    if not set(required) <= set(raw) or not set(raw) <= set(properties):
        return False
    return all(isinstance(value, str) if properties[key]["type"] == "string"
               else isinstance(value, list) and all(isinstance(item, str) for item in value)
               for key, value in raw.items())


def inspect_trace(trace):
    """Require the recorded offered schemas, not a guess from model responses."""
    requests, schemas_hash, awaiting_reply = 0, None, False
    observations = counts()
    for index, event in enumerate(trace):
        kind = event["kind"]
        if kind == "request":
            if awaiting_reply:
                raise ValueError("new request follows an unanswered inference")
            system = event["value"][0]["content"]
            if ("The native adapter has already registered this task" not in system
                    or "For work requiring tools, first call task_begin" in system):
                raise ValueError("frozen-task instruction absent or contradictory")
            if index + 1 >= len(trace) or trace[index + 1]["kind"] != "tool_schemas":
                raise ValueError("request lacks offered tool schemas")
            requests += 1
            awaiting_reply = True
        elif kind == "tool_schemas":
            if index == 0 or trace[index - 1]["kind"] != "request":
                raise ValueError("orphan or duplicate tool schemas")
            offered = event["value"]
            functions = {schema["function"]["name"]: schema["function"] for schema in offered}
            if len(offered) != len(FIELDS) or set(functions) != set(FIELDS):
                raise ValueError("study tool allowlist differs from frozen protocol")
            for schema in offered:
                function = schema["function"]
                properties, required = FIELDS[function["name"]]
                parameters = dict(type="object", properties=properties, required=required, additionalProperties=False)
                if schema["type"] != "function" or function["parameters"] != parameters:
                    raise ValueError("study tool argument schema differs from frozen protocol")
            current = evidence.digest(offered)
            if schemas_hash is not None and schemas_hash != current:
                raise ValueError("offered schemas changed between requests")
            schemas_hash = current
        elif kind == "reply":
            if not awaiting_reply:
                raise ValueError("reply lacks its inference request")
            awaiting_reply = False
            calls = event["value"].get("tool_calls", [])
            observations["multiple_tool_replies"] += len(calls) > 1
            for call in calls:
                function = call.get("function", {})
                name = function.get("name")
                observations["contract_change_calls"] += name in ("task_begin", "task_revise")
                observations["unavailable_tool_calls"] += name not in FIELDS
                if name in FIELDS:
                    observations["invalid_argument_calls"] += not valid_arguments(name, function.get("arguments"))
        else:
            raise ValueError("unknown protocol trace event")
    if not requests or schemas_hash is None:
        raise ValueError("study schema evidence missing")
    return dict(requests=requests, schema_sha256=schemas_hash, observations=observations)


def inspect_original(original):
    result = inspect_trace(original["visible_trace"])
    observations = result["observations"]
    replies = [item["value"] for item in original["visible_trace"] if item["kind"] == "reply"]
    observations["premature_prose_endings"] = int(bool(replies) and not replies[-1].get("tool_calls")
                                                 and original["trial"]["inconclusive"])
    rows = sorted((row for row in original["journal"].values() if row["task"] == original["trial"]["task"]),
                  key=lambda row: row["sequence"])
    task = next(row["event"]["task"] for row in rows if row["event"]["kind"] == "begin")
    writes = {row["event"]["action"] for row in rows
              if row["event"]["kind"] == "action_started" and row["event"]["operation"] == "write"}
    last_write = max((row["sequence"] for row in rows if row["event"]["kind"] == "action_completed"
                      and row["event"]["action"] in writes and row["event"]["outcome"] == "success"), default=None)
    if last_write is not None:
        checked = {row["event"]["criterion"] for row in rows
                   if row["event"]["kind"] == "checked" and row["sequence"] > last_write}
        observations["writes_missing_required_checks"] = int(not {
            criterion["id"] for criterion in task["definition"]["criteria"]} <= checked)
    return result


def check_run(run, deadline):
    result = dict(valid=False, originals=0, requests=0, schema_sha256=None,
                  calibration=counts(), trials=counts(), errors=[])
    for filename in [*run.calibration_originals, *run.originals]:
        evidence.check_deadline(deadline)
        path = Path(filename)
        try:
            original = json.loads(path.read_bytes())
            # Calibration reuses one store; each original embeds its own trace.
            if path.name == "trial.json" and json.loads((path.parent / "visible-trace.json").read_bytes()) != original["visible_trace"]:
                raise ValueError("original trace differs from captured trace")
            checked = inspect_original(original)
            if result["schema_sha256"] not in (None, checked["schema_sha256"]):
                raise ValueError("offered schemas changed between trials")
            result["schema_sha256"] = checked["schema_sha256"]
            result["originals"] += 1
            result["requests"] += checked["requests"]
            group = "calibration" if original["trial"]["arm"] == "calibration" else "trials"
            for name, count in checked["observations"].items():
                result[group][name] += count
        except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
            result["errors"].append(dict(label=path.parent.name if path.name == "trial.json" else path.stem, detail=str(error)))
    evidence.check_deadline(deadline)
    result["valid"] = result["originals"] > 0 and not result["errors"]
    return result
