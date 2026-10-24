#!/usr/bin/env python3
"""Check exact public files against the extraction ledger, without Git installed."""
import hashlib,json
from pathlib import Path
root=Path(__file__).resolve().parent.parent
manifest=json.loads((root/'extraction.json').read_text())
for name,expected in manifest['public_files'].items():
    content=(root/name).read_bytes()
    actual=hashlib.sha1(f'blob {len(content)}\0'.encode()+content).hexdigest()
    assert actual==expected, f'public source bytes changed: {name}'
print(f"Exact public-file parity: {len(manifest['public_files'])} files")
