"""Invoke only the ignored unit-test bridge; never a production model backend."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from s6a_readiness_fixtures import replies


def main():
    args = sys.argv[1:]
    value = json.load(sys.stdin) if "--stdin" in args else None
    # The outer runner supplies process-tree cancellation and the hard deadline.
    with tempfile.TemporaryDirectory(prefix="scripted-call-", dir=os.environ["LUCY_READINESS_BRIDGE_ROOT"]) as temp:
        base = Path(temp)
        request, response = base / "request.json", base / "response.json"
        request.write_text(json.dumps(dict(args=args, input=value,
            replies=replies(value) if args[2:3] == ["study-trial"] else [])))
        env = dict(os.environ, LUCY_READINESS_REQUEST=str(request), LUCY_READINESS_RESPONSE=str(response))
        result = subprocess.run([env["LUCY_READINESS_TEST_BINARY"], "--exact",
            "task_model::readiness::scripted_bridge", "--ignored", "--nocapture"], env=env, capture_output=True)
        if result.returncode:
            sys.stderr.buffer.write(result.stderr)
            return 2
        result = json.loads(response.read_bytes())
        if "error" in result:
            print(result["error"], file=sys.stderr)
            return 2
        print(json.dumps(result["ok"]))
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
