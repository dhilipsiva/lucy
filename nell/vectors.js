// Vectors for recall (roadmap/N2b.md). Every public record and direct-memory fact is placed
// in a vector space kept in the browser's IndexedDB, so recall can find a record by meaning
// and not only by exact text. Two spaces: a hashed space that needs no download (tokens,
// bigrams and character trigrams, weighted by rarity, hashed into a sparse signed space;
// the one the Node test runs), and a neural space from a small embedding model in a worker (embed-worker.js).
// Either ranks records that exist; neither writes a word.

export const HASH_SPACE = 'hash-v2';
export const HASH_DIM = 1 << 20;  // sparse: only the buckets a text touches are kept, so collisions are rare
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

/** Unit length, for a dense Float32Array or a sparse {idx, val} vector. */
export function normalize(v) {
  const val = v.val || v;
  let s = 0; for (const x of val) s += x * x;
  s = Math.sqrt(s) || 1;
  for (let i = 0; i < val.length; i++) val[i] /= s;
  return v;
}
/** Dot product of two unit vectors of the same kind: dense with dense, sparse with sparse. */
export function dot(a, b) {
  if (a.idx && b.idx) {
    let s = 0, i = 0, j = 0;
    while (i < a.idx.length && j < b.idx.length) {
      if (a.idx[i] === b.idx[j]) { s += a.val[i] * b.val[j]; i++; j++; }
      else if (a.idx[i] < b.idx[j]) i++; else j++;
    }
    return s;
  }
  if (a.idx || b.idx || a.length !== b.length) return NaN;
  let s = 0; for (let i = 0; i < a.length; i++) s += a[i] * b[i];
  return s;
}
export const cosine = dot;
export const compatible = (a, b) => (a.idx && b.idx) ? true : (!a.idx && !b.idx && a.length === b.length);

/** Feature weights of a text: its tokens, their bigrams, and character trigrams (so "birds" sits near "bird"). */
export function features(text) {
  const toks = tokenize(text);
  const f = new Map();
  const add = (k, w) => f.set(k, (f.get(k) || 0) + w);
  for (const t of toks) add(t, 1);
  for (let i = 0; i + 1 < toks.length; i++) add(`${toks[i]} ${toks[i + 1]}`, 0.8);
  for (const t of toks) { const w = `_${t}_`; for (let i = 0; i + 3 <= w.length; i++) add(`#${w.slice(i, i + 3)}`, 0.35); }
  return f;
}

/**
 * The hashed space: no model, no download, deterministic. Features are weighted by their
 * rarity in the corpus (inverse document frequency) and hashed into signed buckets, so a
 * rare word such as a bird's name counts for more than a common trigram. The space's id
 * carries a fingerprint of the corpus: a changed corpus is a new space, rebuilt in
 * milliseconds, and older ones are pruned from the store.
 */
export class HashSpace {
  constructor({ dim = HASH_DIM } = {}) { this.dim = dim; this.df = new Map(); this.n = 0; this.id = HASH_SPACE; }
  fit(texts) {
    this.df = new Map(); this.n = texts.length;
    let fp = 0x9e3779b9;
    for (const t of texts) {
      for (const k of features(t).keys()) this.df.set(k, (this.df.get(k) || 0) + 1);
      fp = (fp ^ fnv1a(t)) >>> 0;
    }
    this.id = `${HASH_SPACE}@${this.n}-${fp.toString(16)}`;
    return this;
  }
  idf(k) { return Math.log((this.n + 1) / ((this.df.get(k) || 0) + 1)) + 1; }
  /** A sparse unit vector: sorted bucket indices and their signed, rarity-weighted values. */
  embed(text) {
    const buckets = new Map();
    for (const [k, w] of features(text)) {
      const h = fnv1a(k);
      const b = (h >>> 1) % this.dim;
      buckets.set(b, (buckets.get(b) || 0) + ((h & 1) ? 1 : -1) * w * this.idf(k));
    }
    const idx = Uint32Array.from([...buckets.keys()].sort((x, y) => x - y));
    const val = Float32Array.from(idx, (b) => buckets.get(b));
    return normalize({ idx, val, dim: this.dim });
  }
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
  /** Drop stored spaces that share a prefix with this one but are not it: older corpora of the hashed space. */
  async pruneOthers(prefix) {
    if (!this.db) return 0;
    return new Promise((resolve, reject) => {
      const tx = this.db.transaction('vectors', 'readwrite');
      let n = 0;
      const req = tx.objectStore('vectors').openCursor();
      req.onsuccess = () => {
        const c = req.result;
        if (!c) return;
        if (String(c.value.space).startsWith(prefix) && c.value.space !== this.space) { c.delete(); n += 1; }
        c.continue();
      };
      tx.oncomplete = () => resolve(n);
      tx.onerror = () => reject(tx.error);
    });
  }
  /** Nearest stored vectors to q, best first, none below the floor. */
  search(q, k = 8, floor = 0) {
    const out = [];
    for (const [key, { vec }] of this.vectors) {
      if (!compatible(q, vec)) continue;
      const score = dot(q, vec);
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
