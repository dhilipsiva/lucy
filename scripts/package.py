#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
root=Path(__file__).resolve().parent.parent
version=tomllib.loads((root/'Cargo.toml').read_text())['workspace']['package']['version']
target=os.environ['TARGET']
windows='windows' in target
binary=root/'target'/target/'release'/('lucy.exe' if windows else 'lucy')
subprocess.run([str(binary),'--version'],check=True)
with tempfile.TemporaryDirectory() as data:
    env={**os.environ,'LUCY_DATA_DIR':data}
    subprocess.run([str(binary),'doctor'],env=env,check=True)
if 'linux' in target:
    result=subprocess.run(['file',str(binary)],capture_output=True,text=True,check=True)
    if 'statically linked' not in result.stdout and 'static-pie linked' not in result.stdout:
        raise SystemExit('Linux release must be static; no Nix loader')
out=root/'dist'; out.mkdir(exist_ok=True)
name=f'lucy-v{version}-{target}'
if windows:
    with zipfile.ZipFile(out/f'{name}.zip','w',zipfile.ZIP_DEFLATED) as archive:
        archive.write(binary,'lucy.exe')
else:
    with tarfile.open(out/f'{name}.tar.gz','w:gz') as archive:
        archive.add(binary,arcname='lucy')
