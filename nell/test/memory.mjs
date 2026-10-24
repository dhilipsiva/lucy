// N6's parser test: the room's parser reads the same files the desktop holds, and
// finds every record line; recall matches exact text and never invents.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { parseRecords, recall, learned } from '../memory.js';

const here = dirname(fileURLToPath(import.meta.url));
const text = readFileSync(join(here, '..', '..', 'lucy', 'interactions.nibli'), 'utf8');
const lines = text.split('\n').filter((l) => l.startsWith('record("')).length;
const records = parseRecords(text).map((r) => ({ ...r, peer: 'lucy' }));
let failed = false;
if (records.length !== lines) { console.error(`parsed ${records.length} of ${lines} record lines`); failed = true; }
const ids = new Set(records.map((r) => r.id));
for (const id of ['ClaudeLucy-683296e1-de3f-4380-8d64-55ffd46a67fa-claim-18', 'ClaudeLucy-1b1a9836-dfd6-41a7-890b-f40970ba1731-claim-2']) {
  if (!ids.has(id)) { console.error(`missing ${id}`); failed = true; }
}
records.sort((a, b) => String(b.payload.timestamp || '').localeCompare(String(a.payload.timestamp || '')));
const hit = recall(records, 'Pity is a Sun Conure');
if (!hit.length || !hit[0].payload.text.includes('Pity is a Sun Conure')) { console.error('recall missed a known text'); failed = true; }
if (recall(records, 'xq-no-such-word-zz').length !== 0) { console.error('recall invented a match'); failed = true; }
const l = learned(records, 5);
if (l.length !== 5 || !l.every((r) => ['note', 'claim', 'decision', 'summary'].includes(r.payload.kind))) { console.error('learned() returned the wrong kinds'); failed = true; }
console.log(JSON.stringify({ record_lines: lines, parsed: records.length, newest_learned: l[0].payload.timestamp, recall_hits: hit.length }));
process.exit(failed ? 1 : 0);
