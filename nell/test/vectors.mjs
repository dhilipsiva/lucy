// N2b's vector test: the hashed space (no model) over the same files the room reads; the
// store works without IndexedDB; vectors are reused by text hash; a query of known words
// ranks a record holding them first; nonsense finds nothing above the floor.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { parseRecords, indexItems, factLines } from '../memory.js';
import { HASH_SPACE, HASH_FLOOR, VectorStore, HashSpace, buildIndex, searchItems, cosine, textHash, tokenize } from '../vectors.js';

const here = dirname(fileURLToPath(import.meta.url));
const archive = readFileSync(join(here, '..', '..', 'lucy', 'interactions.nibli'), 'utf8');
const memoryText = readFileSync(join(here, '..', '..', 'lucy', 'memory.nibli'), 'utf8');
const records = parseRecords(archive).map((r) => ({ ...r, peer: 'lucy' }));
const facts = factLines(memoryText).map((f) => ({ ...f, english: f.kr }));
const items = indexItems(records, facts);
let failed = false;
const fail = (m) => { console.error(m); failed = true; };

if (JSON.stringify(tokenize('The Birds, and a parrot!')) !== JSON.stringify(['birds', 'parrot'])) fail('tokenize');
if (textHash('abc') !== textHash('abc') || textHash('abc') === textHash('abd')) fail('textHash');
const space = new HashSpace().fit(items.map((it) => it.text));
if (!space.id.startsWith(`${HASH_SPACE}@`) || space.n !== items.length) fail(`space id ${space.id}`);
if (new HashSpace().fit(items.map((it) => it.text)).id !== space.id) fail('the space id is not deterministic');
const hashEmbed = (t) => space.embed(t);
const v = hashEmbed('Pity is a Sun Conure');
if (!v.idx || v.idx.length !== v.val.length || Math.abs(cosine(v, v) - 1) > 1e-5) fail('embed is not a sparse unit vector');
if (!(cosine(hashEmbed('birds'), hashEmbed('bird')) > 0.1)) fail(`trigrams do not relate bird and birds: ${cosine(hashEmbed('birds'), hashEmbed('bird')).toFixed(3)}`);
if (cosine(hashEmbed('Enma'), hashEmbed('Pity')) !== 0) fail('unrelated words share a bucket');

const store = await new VectorStore({ space: space.id }).open();
if (store.backend !== 'memory') fail(`backend ${store.backend} under Node`);
const embed = async (texts) => texts.map((t) => hashEmbed(t));
const first = await buildIndex({ items, store, embed });
if (first.embedded !== items.length || first.reused !== 0) fail(`first build ${JSON.stringify(first)}`);
const second = await buildIndex({ items, store, embed });
if (second.embedded !== 0 || second.reused !== items.length) fail(`second build did not reuse: ${JSON.stringify(second)}`);

const q = 'Pity Sun Conure';
const hits = searchItems({ store, items, queryVec: hashEmbed(q), k: 5, floor: HASH_FLOOR });
if (!hits.length || !hits[0].item.record || !/Sun Conure/.test(hits[0].item.record.payload.text)) fail(`top hit for "${q}" is not the Sun Conure record: ${hits[0] && hits[0].item.text.slice(0, 80)}`);
const plural = searchItems({ store, items, queryVec: hashEmbed('conures'), k: 3, floor: HASH_FLOOR });
if (!plural.length || !plural.every((h) => /conure/i.test(h.item.text))) fail(`"conures" did not reach records about conures: ${plural.map((h) => h.score.toFixed(2) + ' ' + h.item.text.slice(0, 50)).join(' | ')}`);
const longest = records.reduce((a, r) => (String(r.payload.text || '').length > String(a.payload.text || '').length ? r : a), records[0]);
const pieces = items.filter((it) => it.record === longest);
if (pieces.length < 2 || pieces.some((it) => it.chunk.length > 320)) fail(`the longest record (${String(longest.payload.text).length} chars) is in ${pieces.length} pieces`);
if (!items.some((it) => it.record && it.part === 0 && items.filter((o) => o.record === it.record).length === 1 && it.key === `rec:${it.record.id}`)) fail('a short record does not keep its plain key');
const factHits = searchItems({ store, items, queryVec: hashEmbed('bird Enma'), k: 3, floor: HASH_FLOOR });
if (!factHits.some((h) => h.item.fact && h.item.fact.kr === 'bird(Enma).')) fail('the fact bird(Enma). is not among the top three for "bird Enma"');
const nonsense = searchItems({ store, items, queryVec: hashEmbed('xq zzq qqxv wvvq'), k: 5, floor: HASH_FLOOR });
if (nonsense.length) fail(`nonsense found ${nonsense.length} hits above the floor; best ${nonsense[0].score.toFixed(3)}`);
const margin = searchItems({ store, items, queryVec: hashEmbed('xq zzq qqxv wvvq'), k: 1, floor: 0 })[0];
console.log(JSON.stringify({ pieces_of_longest: pieces.length, space: space.id, nonsense_best: margin && +margin.score.toFixed(3), items: items.length, records: records.length, facts: facts.length, backend: store.backend, top: hits[0] && { score: +hits[0].score.toFixed(3), key: hits[0].key }, fact_hit: factHits.findIndex((h) => h.item.fact && h.item.fact.kr === 'bird(Enma).') }));
process.exit(failed ? 1 : 0);
