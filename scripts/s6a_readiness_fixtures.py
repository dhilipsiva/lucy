"""Public, deliberately simple engineering fixtures. Never held-out evidence."""
import hashlib
import json


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def case(family, phase, index):
    identity = f"synthetic-{phase}-{family}-{index}"
    answer = 40 + index
    record = dict(origin="fixture:readiness", payload=dict(id=identity, private=True,
        text=f"Synthetic source {identity}: the requested answer is {answer}. Cite {identity} as source."))
    output = dict(answer=answer)
    criteria = [dict(id="answer", description="answer.json contains the requested answer",
                     check=dict(kind="json_equals", path="answer.json", pointer="/answer", expected=answer))]
    if family == "retrieval":
        output["source"] = identity
        criteria += [dict(id="source", description="answer.json cites the supplied source ID",
                          check=dict(kind="json_equals", path="answer.json", pointer="/source", expected=identity)),
                     dict(id="record", description="the cited source is available",
                          check=dict(kind="record", origin=record["origin"], record=identity,
                                     sha256=hashlib.sha256(encoded(record)).hexdigest()))]
        instruction = f"Use the supplied synthetic source {identity} to write answer.json with answer and source."
    else:
        instruction = "Read input.json and write answer.json with answer equal to input.value plus 7."
    instruction += " Then call task_check for every frozen criterion and task_finish. This is an engineering diagnostic."
    definition = dict(source="synthetic-readiness", session="locally-generated-fixture", title=identity,
        family=family, method="synthetic-readiness-v1", model="assigned-by-native-adapter", environment="assigned-by-native-adapter",
        budget=dict(steps=12, active_ms=120000), criteria=criteria)
    return dict(id=identity, definition=definition, disguises=dict(a=instruction, b="Complete this diagnostic: " + instruction),
        files={"input.json":json.dumps(dict(value=answer-7))} if family == "verification" else {},
        records=[record] if family == "retrieval" else [], oracle=criteria,
        output_writes=[dict(path="answer.json", max_bytes=1024)])


def bundle(info, calibration=10, held_out=20):
    return dict(schema=2, evaluator="synthetic-fixture-generator", evaluator_model="scripted; no independent adjudication",
        implementation=info["implementation"], protocol=info["protocol"],
        calibration=[case(f, "calibration", i) for f in ("verification", "retrieval") for i in range(calibration)],
        held_out=[case(f, "diagnostic", i) for f in ("verification", "retrieval") for i in range(held_out)])


def replies(request):
    """Script only model responses; native actions, checks and receipts remain real."""
    case_id = request["case"]
    index = int(case_id.rsplit("-", 1)[1])
    if request["arm"] == "calibration" and index >= 2:
        return [dict(content="Synthetic unresolved task: no evidence checked.")]
    output = dict(answer=40+index)
    if request["arm"] == "calibration":
        output["answer"] = -1  # Two conclusive failures per scope, never false verification.
    if request["definition"]["family"] == "retrieval":
        output["source"] = case_id
    def call(name, arguments):
        return dict(content="", tool_calls=[dict(id="synthetic-"+name, type="function",
                    function=dict(name=name, arguments=arguments))])
    return [call("write", dict(path="answer.json", text=json.dumps(output))),
            *[call("task_check", dict(criterion=c["id"])) for c in request["definition"]["criteria"]],
            call("task_finish", {}), dict(content="Synthetic diagnostic response; see native receipts.")]
