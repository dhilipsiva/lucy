#!/usr/bin/env python3
"""Sets the line numbers of the scenes that show vidni.toml itself.

The video shows the file it is made from, with the line being said marked.
Those scenes name lines of the file by number, and the numbers move when
the script is edited. Run this after `vidni fmt`; it changes digits only,
never the number of lines, and says so if it can't.

    python3 tools/self.py vidni.toml
"""
import re
import sys

path = sys.argv[1]
text = open(path).read()
lines = text.split("\n")

# This introduction uses cards and the room image. Earlier scripts showed
# their own source; keep this helper safe for either layout.
if not re.search(r"^\[scenes\.(self|self-narrow|voice|voice-narrow)\]$", text, re.M):
    print(f"{path}: no self-source scenes; nothing to update")
    sys.exit(0)


def table(start_pattern):
    """The first and last line (from 1) of the table that holds a line matching the pattern."""
    at = next(i for i, line in enumerate(lines) if re.fullmatch(start_pattern, line))
    first = at
    while first > 0 and lines[first - 1].strip() and not lines[first - 1].startswith("["):
        first -= 1
    if first > 0 and lines[first - 1].startswith("["):
        first -= 1
    last = at
    while last + 1 < len(lines) and lines[last + 1].strip():
        last += 1
    return first + 1, last + 1


watching = table(r'id = "made-1"')
now = table(r'id = "made-2"')
voice = table(r"draft = .*")
said = next(i + 1 for i, line in enumerate(lines) if line.startswith('draft = "'))
wanted = {
    "self": {"lines": (watching[0], now[1]), "watching": watching, "now": now},
    "self-narrow": {"lines": (watching[0], now[1]), "watching": watching, "now": now},
    "voice": {"lines": voice, "draft": (said, said)},
    "voice-narrow": {"lines": voice, "draft": (said, said)},
}
scene = None
for i, line in enumerate(lines):
    head = re.fullmatch(r"\[scenes\.([a-z-]+)(\.targets)?\]", line)
    if head:
        scene = head.group(1)
        continue
    if line.startswith("["):
        scene = None
    key = re.match(r"([a-z]+) = ", line)
    if scene in wanted and key and key.group(1) in wanted[scene]:
        a, b = wanted[scene][key.group(1)][:2]
        lines[i] = re.sub(r"\[\d+, \d+\]", f"[{a}, {b}]", line)
after = "\n".join(lines)
if after.count("\n") != text.count("\n"):
    sys.exit("the number of lines changed; nothing written")
open(path, "w").write(after)
print(f"{path}: self {watching[0]}..{now[1]}, voice {voice[0]}..{voice[1]}")
