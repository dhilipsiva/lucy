#!/usr/bin/env python3
"""Publish only explicitly public registry archives beside the room, without credentials."""
import argparse
import json
from pathlib import Path
import re
import shutil
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
QUOTED = r'"(?:[^"\\]|\\.)*"'
RECORD = re.compile(r'record\((' + QUOTED + r'),\s*(' + QUOTED + r'),\s*(Message|Note|Summary|LegacyJournal|Claim|Decision),\s*Json\)\.')

def validate(text):
    count = 0
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if not line.startswith('record('):
            if line.startswith(('message(', 'date(', 'source(', 'claim(', 'expresses(', 'member(')):
                continue
            raise ValueError('Unsupported public archive statement')
        match = RECORD.fullmatch(line)
        if not match:
            raise ValueError('Unsupported public archive record')
        identity, payload = json.loads(match[1]), json.loads(json.loads(match[2]))
        if payload.get('id') != identity or not isinstance(payload.get('text'), str):
            raise ValueError('Invalid public archive identity/text')
        if payload.get('private') is not False or payload.get('channel') in ('analysis', 'reasoning', 'thinking') or payload.get('role') in ('system', 'developer', 'tool'):
            raise ValueError('Nonpublic or hidden record refused')
        count += 1
    return count

def build(destination, local_only=False):
    registry = json.loads((ROOT / 'peers.json').read_text())
    peers = registry['peers']
    if registry['version'] != 1:
        raise ValueError('Unsupported registry')
    destination.mkdir(parents=True, exist_ok=True)
    for peer in peers:
        if peer['public'] is not True or peer.get('credential_env') or peer.get('credential_file') or peer.get('local_path') or peer.get('record_origins'):
            raise ValueError('Only the public origin registry can be published')
        if not re.fullmatch(r'[a-zA-Z0-9_-]+', peer['name']) or not re.fullmatch(r'[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+', peer['repository']):
            raise ValueError('Invalid public peer path')
        if peer['name'] == 'lucy':
            text = (ROOT / 'lucy/interactions.nibli').read_text()
        elif local_only:
            continue
        else:
            path = next(p for p in peer['source_paths'] if p == 'lucy/interactions.nibli')
            revision = urllib.parse.quote(peer['source_ref'], safe='')
            url = f"https://raw.githubusercontent.com/{peer['repository']}/{revision}/{path}"
            with urllib.request.urlopen(url, timeout=30) as response:
                text = response.read().decode('utf-8')
        count = validate(text)
        folder = destination / 'public-peers' / peer['name']
        folder.mkdir(parents=True, exist_ok=True)
        (folder / 'interactions.nibli').write_text(text)
        print(f"{peer['name']}: {count} public records")
    (destination / 'peers.json').write_text(json.dumps(registry, indent=2) + '\n')
    (destination / 'lucy').mkdir(exist_ok=True)
    shutil.copyfile(ROOT / 'lucy/interactions.nibli', destination / 'lucy/interactions.nibli')

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('destination', type=Path)
    parser.add_argument('--local-only', action='store_true')
    args = parser.parse_args()
    build(args.destination, args.local_only)
