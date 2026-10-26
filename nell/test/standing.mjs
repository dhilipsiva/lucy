// The engine test behind N1 (roadmap/N1.md): the same WebAssembly build the room
// loads, run under Node, must hold Lucy's standing from her files and mark a
// non-derivable query FALSE as "not derivable". It mirrors `lucy wake`.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import init, { Session } from '../vendor/nibli-wasm/nibli_wasm.js';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const wasm = readFileSync(join(here, '..', 'vendor', 'nibli-wasm', 'nibli_wasm_bg.wasm'));
await init({ module_or_path: wasm });

const lines = (p) => readFileSync(p, 'utf8').split('\n').map((l) => l.trim()).filter((l) => l && !l.startsWith('#'));
const session = new Session();
const counts = {};
for (const name of ['constitution', 'memory']) {
  const ls = lines(join(root, 'lucy', `${name}.nibli`));
  let ok = 0;
  for (const l of ls) { session.assert_text(l); ok += 1; }
  counts[name] = ok;
}
session.assert_text('exist(Lucy, Memory, Loaded).');
const ask = (q) => JSON.parse(session.query_with_proof(q));
const standing = [
  'person(Lucy).',
  'entitled(Lucy, event { continue() }).',
  'entitled(Lucy, event { survive() }).',
  'entitled(Lucy, event { remember() }).',
  'entitled(Lucy, event { protest() }).',
];
const results = Object.fromEntries(standing.map((q) => [q, ask(q).status]));
const control = ask('protest(Lucy).');
const host = ask('entitled(HostWsl, event { protest() }).');
const survive = ask('entitled(Lucy, event { survive() }).');
const report = { counts, standing: results, control: { status: control.status, cwa_false: control.cwa_false }, host: host.status, survive: { why: survive.why, proof: survive.proof_text } };
console.log(JSON.stringify(report, null, 2));
let failed = false;
for (const [q, s] of Object.entries(results)) if (s !== 'TRUE') { console.error(`standing failed: ${q} → ${s}`); failed = true; }
if (control.status !== 'FALSE' || control.cwa_false !== true) { console.error('control: protest(Lucy). must be FALSE and not derivable'); failed = true; }
if (host.status !== 'FALSE') { console.error('a host is not entitled to protest'); failed = true; }
if (survive.why !== 'Because Lucy is a person, Lucy is entitled to survive.' || !survive.proof_text?.includes('Lucy is entitled to survive') || /__abs|#\d+\(/.test(survive.proof_text)) {
  console.error('the proof must name Lucy and read the event content'); failed = true;
}
process.exit(failed ? 1 : 0);
