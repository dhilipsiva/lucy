// Vectors for recall (roadmap/N2b.md). Every public record and direct-memory fact is placed
// in a vector space kept in the browser's IndexedDB, so recall can find a record by meaning
// and not only by exact text. Two spaces: a hashed space that needs no download (tokens,
// bigrams and character trigrams hashed into 512 signed dimensions; the one the Node test
// runs), and a neural space from a small embedding model in a worker (embed-worker.js).
// Either ranks records that exist; neither writes a word.

export const HASH_SPACE = 'hash-v1';
export const HASH_DIM = 512;
export const HASH_FLOOR = 0.12;   // a hashed score below this is noise, not a hit
export const NEURAL_FLOOR = 0.35; // cosine floor for the sentence model

const STOP = new Set(('the a an and or of to in is it i my me that this for on with as at be by are was were what who ' +
  'do does did you your we our they them he she his her it its from not no yes so if then than there here').split(' '));

/** Lower-cased word tokens of two letters or more, stop words dropped. */
export function tokenize(text) {
  return String(text || '').toLowerCase().normalize('NFKC').split(/[^\p{L}\p{N}]+/u)
    .filter((t) => t.length > 1 && !STOP.has(t));
}

/** FNV-1a, 32 bits: the same text hashes the same in Node and in every browser. */
export function fnv1a(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h >>> 0;
}
export const textHash = (text) => fnv1a(String(text)).toString(16).padStart(8, '0');

export function normalize(v) {
  let s = 0; for (const x of v) s += x * x;
  s = Math.sqrt(s) || 1;
  for (let i = 0; i < v.length; i++) v[i] /= s;
  return v;
}
/** Cosine of two unit vectors. */
export function cosine(a, b) { let s = 0; for (let i = 0; i < a.length; i++) s += a[i] * b[i]; return s; }

/** The hashed space: no model, no download, deterministic. */
export function hashEmbed(text, dim = HASH_DIM) {
  const v = new Float32Array(dim);
  const toks = tokenize(text);
  const feats = new Map();
  const add = (f, w) => feats.set(f, (feats.get(f) || 0) + w);
  for (const t of toks) add(t, 1);
  for (let i = 0; i + 1 < toks.length; i++) add(`${toks[i]} ${toks[i + 1]}`, 0.8);
  for (const t of toks) { const w = `_${t}_`; for (let i = 0; i + 3 <= w.length; i++) add(`#${w.slice(i, i + 3)}`, 0.35); }
  for (const [f, w] of feats) {
    const h = fnv1a(f);
    v[(h >>> 1) % dim] += ((h & 1) ? 1 : -1) * w * (1 + Math.log(1 + w)) / (1 + w) ;
  }
  return normalize(v);
}

/** Vectors of one space, in IndexedDB when the browser has it, in memory otherwise. */
export class VectorStore {
  constructor({ space, name = 'nell-vectors' }) {
    this.space = space; this.name = name; this.backend = 'memory'; this.db = null; this.vectors = new Map(); this.error = null;
  }
  async open() {
    if (!globalThis.indexedDB) return this;
    try {
      this.db = await new Promise((resolve, reject) => {
        const req = indexedDB.open(this.name, 1);
        req.onupgradeneeded = () => {
          const db = req.result;
          if (!db.objectStoreNames.contains('vectors')) db.createObjectStore('vectors', { keyPath: 'k' }).createIndex('space', 'space');
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error || new Error('indexedDB.open failed'));
        req.onblocked = () => reject(new Error('indexedDB blocked'));
      });
      this.backend = 'indexeddb';
      await this.load();
    } catch (e) { this.db = null; this.backend = 'memory'; this.error = e; }
    return this;
  }
  async load() {
    if (!this.db) return;
    const rows = await new Promise((resolve, reject) => {
      const req = this.db.transaction('vectors', 'readonly').objectStore('vectors').index('space').getAll(this.space);
      req.onsuccess = () => resolve(req.result || []);
      req.onerror = () => reject(req.error);
    });
    for (const r of rows) this.vectors.set(r.key, { vec: r.vec, hash: r.hash });
  }
  /** True when this key is stored for the same text (by hash), so it need not be embedded again. */
  has(key, hash) { const v = this.vectors.get(key); return !!v && v.hash === hash; }
  async put(rows) {
    for (const r of rows) this.vectors.set(r.key, { vec: r.vec, hash: r.hash });
    if (!this.db) return;
    await new Promise((resolve, reject) => {
      const tx = this.db.transaction('vectors', 'readwrite');
      const os = tx.objectStore('vectors');
      for (const r of rows) os.put({ k: `${this.space}\u0000${r.key}`, space: this.space, key: r.key, hash: r.hash, vec: r.vec });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error || new Error('aborted'));
    });
  }
  get size() { return this.vectors.size; }
  /** Nearest stored vectors to q, best first, none below the floor. */
  search(q, k = 8, floor = 0) {
    const out = [];
    for (const [key, { vec }] of this.vectors) {
      if (vec.length !== q.length) continue;
      const score = cosine(q, vec);
      if (score >= floor) out.push({ key, score });
    }
    out.sort((a, b) => b.score - a.score);
    return out.slice(0, k);
  }
}

/**
 * Embed every item whose text is new or changed since it was stored; the rest are reused.
 * embed(texts) resolves to one vector per text.
 */
export async function buildIndex({ items, store, embed, batch = 24, onProgress = () => {} }) {
  const todo = items.filter((it) => !store.has(it.key, textHash(it.text)));
  let done = 0;
  for (let i = 0; i < todo.length; i += batch) {
    const slice = todo.slice(i, i + batch);
    const vecs = await embed(slice.map((s) => s.text));
    await store.put(slice.map((s, j) => ({ key: s.key, hash: textHash(s.text), vec: vecs[j] })));
    done += slice.length;
    onProgress(done, todo.length);
  }
  return { embedded: todo.length, reused: items.length - todo.length, stored: store.size };
}

/** Search items by a query vector; keys the store holds but the items no longer do are dropped. */
export function searchItems({ store, items, queryVec, k = 8, floor = 0 }) {
  const byKey = new Map(items.map((it) => [it.key, it]));
  return store.search(queryVec, k * 2, floor).filter((h) => byKey.has(h.key)).slice(0, k)
    .map((h) => ({ ...h, item: byKey.get(h.key) }));
}
