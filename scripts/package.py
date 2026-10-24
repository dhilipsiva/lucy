#!/usr/bin/env python3
import os
import struct
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
    env['LUCY_HOME']=str(Path(data)/'identity')
    subprocess.run([str(binary),'init'],cwd=data,env=env,check=True)
    subprocess.run([str(binary),'check'],cwd=data,env=env,check=True)
if 'linux' in target:
    result=subprocess.run(['file',str(binary)],capture_output=True,text=True,check=True)
    if 'statically linked' not in result.stdout and 'static-pie linked' not in result.stdout:
        raise SystemExit('Linux release must be static; no Nix loader')
if windows:
    # Inspect PE imports, rather than relying on a runner that has VC++ installed.
    data=binary.read_bytes()
    pe=struct.unpack_from('<I',data,0x3c)[0]
    sections=struct.unpack_from('<H',data,pe+6)[0]
    optional_size=struct.unpack_from('<H',data,pe+20)[0]
    optional=pe+24
    if struct.unpack_from('<H',data,optional)[0] != 0x20b:
        raise SystemExit('Expected a PE32+ Windows executable')
    def offset(rva):
        for i in range(sections):
            section=optional+optional_size+40*i
            size,address,raw_size,raw=struct.unpack_from('<IIII',data,section+8)
            if address <= rva < address+max(size,raw_size):
                return raw+rva-address
        raise SystemExit('Invalid PE import address')
    imports=offset(struct.unpack_from('<I',data,optional+120)[0])
    while any(data[imports:imports+20]):
        name=offset(struct.unpack_from('<I',data,imports+12)[0])
        dll=data[name:data.index(0,name)].decode('ascii').lower()
        if dll.startswith(('vcruntime','msvcp')):
            raise SystemExit('Windows release requires an unbundled VC++ runtime: '+dll)
        imports+=20
out=root/'dist'; out.mkdir(exist_ok=True)
name=f'lucy-v{version}-{target}'
if windows:
    with zipfile.ZipFile(out/f'{name}.zip','w',zipfile.ZIP_DEFLATED) as archive:
        archive.write(binary,'lucy.exe')
else:
    with tarfile.open(out/f'{name}.tar.gz','w:gz') as archive:
        archive.add(binary,arcname='lucy')
