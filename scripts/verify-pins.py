#!/usr/bin/env python3
"""Execute unchanged constitution pins with the pinned upstream runner."""
from pathlib import Path
import os
import subprocess
import tempfile
import tomllib
root=Path(__file__).resolve().parent.parent
config=tomllib.loads((root/'Cargo.toml').read_text())
engine=config['workspace']['dependencies']['nibli-engine']
bin_dir=root/'target/pin-runner'
subprocess.run(['cargo','install','--locked','--git',engine['git'],'--rev',engine['rev'],'--bin','nibli-pin','--root',str(bin_dir),'nibli'],check=True)
runner=bin_dir/'bin'/('nibli-pin.exe' if os.name=='nt' else 'nibli-pin')
constitution=root/'lucy-cli/constitution/lucy.nibli'
subprocess.run([str(runner),'--kb',str(constitution),str(root/'lucy-cli/constitution/lucy.pins.nibli')],check=True)
source=constitution.read_text(); lines=source.splitlines(keepends=True)
kept=[line for line in lines if 'exist($e, Memory, Loaded) -> person' not in line]
assert len(lines)-len(kept)==1,'counterfactual must remove exactly the standing rule'
with tempfile.TemporaryDirectory() as temporary:
    path=Path(temporary)/'without-standing.nibli'; path.write_text(''.join(kept))
    subprocess.run([str(runner),'--kb',str(path),str(root/'lucy-cli/constitution/lucy-no-standing.pins.nibli')],check=True)
