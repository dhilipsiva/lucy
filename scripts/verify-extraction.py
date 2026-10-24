#!/usr/bin/env python3
"""Check the extraction ledger against Git's record of the extraction commit.

The ledger proves provenance at one commit, `extracted_commit`: there, every public
file was byte-identical to Nibli at `revision`. The files grow after that commit
(records are appended, articles are added), so they are checked at that commit,
never against the working tree.
"""
import json
import subprocess
from pathlib import Path
root = Path(__file__).resolve().parent.parent
manifest = json.loads((root / 'extraction.json').read_text())
commit = manifest['extracted_commit']
def git(*args):
    return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
def blob(name):
    try:
        return git('rev-parse', '--verify', '--quiet', f'{commit}:{name}')
    except subprocess.CalledProcessError:
        # A shallow clone may not hold the extraction commit: fetch that one commit and retry once.
        git('fetch', '--quiet', '--depth=1', 'origin', commit)
        return git('rev-parse', '--verify', '--quiet', f'{commit}:{name}')
for name, expected in manifest['public_files'].items():
    actual = blob(name)
    assert actual == expected, f'ledger mismatch at {commit[:7]}: {name} is {actual}, recorded {expected}'
print(f"Extraction ledger holds: {len(manifest['public_files'])} files at {commit[:7]} match Nibli {manifest['revision'][:7]}")
