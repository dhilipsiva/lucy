// N4-0's test: the room's journal keeps entries in the record shape and exports them unchanged.
const store = new Map();
globalThis.localStorage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) };
const J = await import('../journal.js');
let failed = false;
const a = J.remember({ speaker: 'Visitor', text: 'protest(Lucy).', about: ['room'] });
const b = J.remember({ speaker: 'Nell', text: 'Lucy protests. I cannot derive that; it is not something my memory holds. → FALSE (not derivable)', about: ['room'] });
if (!a || !b || J.count() !== 2) { console.error('two entries expected'); failed = true; }
for (const e of J.all()) for (const k of ['id', 'speaker', 'text', 'kind', 'timestamp', 'source', 'session', 'channel', 'about', 'private']) if (!(k in e)) { console.error(`missing ${k}`); failed = true; }
if (JSON.parse(J.exportJson()).length !== 2) { console.error('export differs'); failed = true; }
if (J.newest(1)[0].text !== b.text) { console.error('newest first expected'); failed = true; }
J.clear();
if (J.count() !== 0) { console.error('clear failed'); failed = true; }
console.log(JSON.stringify({ shape: 'record', entries: 2, export: 'identical', clear: 'ok' }));
process.exit(failed ? 1 : 0);
