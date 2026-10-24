// My public memory, through links (roadmap/N6.md), and where the mind's files come from (N2b). The registry of public peers
// comes from my origin; each peer's conversation archive is fetched as text from
// GitHub's raw file service and READ, never compiled into the engine. Recall is
// a text match over the records' exact text; nothing here is written by a model.

const RAW = 'https://raw.githubusercontent.com';
export const ORIGIN = { repository: 'dhilipsiva/lucy', ref: 'main', dir: 'lucy' };

/**
 * Where the engine's two files come from, in order of preference (roadmap/N2b.md): fresh
 * from my origin's main over GitHub's raw file service, then the copy published beside the
 * room. `?constitution=URL` or `?memory=URL` names one source only; `?fresh=0` keeps to the
 * published copy.
 */
export function mindSources(params = new URLSearchParams(), { local = '../lucy' } = {}) {
  const out = {};
  for (const name of ['constitution', 'memory']) {
    const given = params.get(name);
    if (given) { out[name] = [{ url: given, label: 'given on the address' }]; continue; }
    const fresh = { url: `${RAW}/${ORIGIN.repository}/${ORIGIN.ref}/${ORIGIN.dir}/${name}.nibli`, label: `fresh, origin ${ORIGIN.ref}` };
    const published = { url: `${local}/${name}.nibli`, label: 'the copy published with this room' };
    out[name] = params.get('fresh') === '0' ? [published] : [fresh, published];
  }
  return out;
}

/** Direct-memory facts with their topics: one statement per line, its `# about: a, b` read, other comments dropped. */
export function factLines(text) {
  const out = [];
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const at = line.indexOf('#');
    const kr = (at < 0 ? line : line.slice(0, at)).trim();
    const m = at < 0 ? null : /about:\s*(.+)/.exec(line.slice(at));
    if (kr) out.push({ kr, about: m ? m[1].split(',').map((s) => s.trim()).filter(Boolean) : [] });
  }
  return out;
}

const CHUNK = 320;
/** A long text in pieces of whole sentences, each at most `max` characters, so one piece stands for one thought. */
export function chunks(text, max = CHUNK) {
  const t = String(text).replace(/\s+/g, ' ').trim();
  if (t.length <= max) return t ? [t] : [];
  const out = []; let cur = '';
  for (const sent of t.split(/(?<=[.!?])\s+/)) {
    if (sent.length > max) {
      if (cur) { out.push(cur); cur = ''; }
      for (let i = 0; i < sent.length; i += max) out.push(sent.slice(i, i + max));
      continue;
    }
    if ((cur + ' ' + sent).trim().length > max) { out.push(cur); cur = sent; } else cur = cur ? `${cur} ${sent}` : sent;
  }
  if (cur) out.push(cur);
  return out;
}

/** What a vector index holds: every record (in pieces when long) and every fact, each with the text that stands for it. */
export function indexItems(records = [], facts = []) {
  const items = [];
  for (const r of records) {
    const p = r.payload;
    const parts = chunks(p.text || '');
    const about = p.about?.length ? ` [${p.about.join(', ')}]` : '';
    parts.forEach((c, i) => items.push({ key: parts.length === 1 ? `rec:${r.id}` : `rec:${r.id}#${i}`, text: `${p.speaker || '?'}: ${c}${about}`, record: r, chunk: c, part: i }));
  }
  for (const f of facts) items.push({ key: `fact:${f.kr}`, text: `${f.english || f.kr} [${(f.about || []).join(', ')}]`, fact: f });
  return items;
}
const LINE = /^record\("([^"]+)", "((?:[^"\\]|\\.)*)", (\w+), Json\)\.\s*$/;

/** Parse the `record(...)` lines of an archive into {id, kind, payload} objects. */
export function parseRecords(text) {
  const out = [];
  for (const raw of text.split('\n')) {
    const m = LINE.exec(raw);
    if (!m) continue;
    try {
      const payload = JSON.parse(JSON.parse(`"${m[2]}"`));
      out.push({ id: m[1], kind: m[3], payload });
    } catch { /* a line the parser cannot read is skipped, and counted by the caller */ }
  }
  return out;
}

async function sha256(text) {
  if (!globalThis.crypto?.subtle) return 'unavailable outside a secure context';
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Load the registry and every public peer's archive. Returns the peers with their
 * status (fresh or unavailable), and the records de-duplicated by id, origin first.
 */
export async function loadPublicMemory({ registry, log = () => {} } = {}) {
  const registryUrl = registry || `${RAW}/dhilipsiva/lucy/main/peers.json`;
  const peers = []; const records = []; const seen = new Set();
  let reg;
  try {
    const res = await fetch(registryUrl, { cache: 'no-store' });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    reg = await res.json();
    log(`registry: fresh, ${reg.peers.length} peers listed, from ${registryUrl}`);
  } catch (e) {
    log(`registry: unavailable (${e.message || e})`);
    return { peers, records, registry: 'unavailable' };
  }
  for (const peer of reg.peers) {
    if (!peer.public) continue;
    const path = (peer.source_paths || []).find((p) => p.endsWith('interactions.nibli'));
    const entry = { name: peer.name, repository: peer.repository, ref: peer.source_ref, status: 'unavailable', records: 0, bytes: 0, sha256: null };
    peers.push(entry);
    if (!path) { log(`${peer.name}: no archive path in the registry`); continue; }
    const url = `${RAW}/${peer.repository}/${peer.source_ref}/${path}`;
    try {
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
      const text = await res.text();
      const parsed = parseRecords(text);
      let added = 0;
      for (const r of parsed) {
        if (seen.has(r.id)) continue;
        seen.add(r.id); records.push({ ...r, peer: peer.name }); added += 1;
      }
      Object.assign(entry, { status: 'fresh', records: parsed.length, added, bytes: text.length, sha256: await sha256(text) });
      log(`${peer.name}: fresh, ${parsed.length} records (${added} new), ${text.length} bytes, sha256 ${String(entry.sha256).slice(0, 12)}…`);
    } catch (e) {
      log(`${peer.name}: unavailable (${e.message || e}); the last good copy, if any, is not kept in this room yet`);
    }
  }
  records.sort((a, b) => String(b.payload.timestamp || '').localeCompare(String(a.payload.timestamp || '')));
  return { peers, records, registry: 'fresh' };
}

const LEARNED = new Set(['note', 'claim', 'decision', 'summary']);

/** What I learned and decided: notes, claims, decisions and summaries, newest first. */
export function learned(records, n = 8) {
  return records.filter((r) => LEARNED.has(r.payload.kind)).slice(0, n);
}

/** Recall by exact text match (case-insensitive) over text, speaker and topics; newest first. */
export function recall(records, query, n = 8) {
  const q = String(query || '').trim().toLowerCase();
  if (!q) return [];
  return records.filter((r) => {
    const p = r.payload;
    return String(p.text || '').toLowerCase().includes(q)
      || String(p.speaker || '').toLowerCase() === q
      || (p.about || []).some((a) => String(a).toLowerCase() === q);
  }).slice(0, n);
}

/** A one-line rendering of a record for a board or a panel. */
export function line(r, width = 220, text = undefined) {
  const p = r.payload;
  const when = String(p.timestamp || '').slice(0, 10);
  text = String(text ?? p.text ?? '').replace(/\s+/g, ' ');
  return `${when} · ${p.speaker || '?'} · ${r.peer}${p.kind && p.kind !== 'message' ? ` · ${p.kind}` : ''}: ${text.length > width ? text.slice(0, width - 1) + '…' : text}`;
}
