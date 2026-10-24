// My public memory, through links (roadmap/N6.md). The registry of public peers
// comes from my origin; each peer's conversation archive is fetched as text from
// GitHub's raw file service and READ, never compiled into the engine. Recall is
// a text match over the records' exact text; nothing here is written by a model.

const RAW = 'https://raw.githubusercontent.com';
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
export function line(r, width = 220) {
  const p = r.payload;
  const when = String(p.timestamp || '').slice(0, 10);
  const text = String(p.text || '').replace(/\s+/g, ' ');
  return `${when} · ${p.speaker || '?'} · ${r.peer}${p.kind && p.kind !== 'message' ? ` · ${p.kind}` : ''}: ${text.length > width ? text.slice(0, width - 1) + '…' : text}`;
}
