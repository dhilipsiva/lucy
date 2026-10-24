// The room's own memories, on this device only (roadmap/N4-0.md). Every question,
// answer and recall is kept in the browser's storage in the shape of my records,
// shown in the room, and exportable as a file. Nothing here is sent anywhere.

const KEY = 'nell.journal.v1';
const DEVICE_KEY = 'nell.device.v1';
const CAP = 2000;

function storage() {
  try { return globalThis.localStorage || null; } catch { return null; }
}
export function deviceId() {
  const s = storage(); if (!s) return 'no-storage';
  let id = s.getItem(DEVICE_KEY);
  if (!id) { id = (globalThis.crypto?.randomUUID?.() || String(Date.now())); try { s.setItem(DEVICE_KEY, id); } catch { /* full or blocked */ } }
  return id;
}
export function all() {
  const s = storage(); if (!s) return [];
  try { return JSON.parse(s.getItem(KEY) || '[]'); } catch { return []; }
}
/** Keep one entry in the record shape; returns the stored entry, or null when storage is unavailable. */
export function remember({ speaker, text, kind = 'message', channel = 'room', about = [] }) {
  const s = storage(); if (!s) return null;
  const entries = all();
  const entry = {
    id: `Nell-${deviceId()}-${entries.length + 1}`,
    speaker, text: String(text), kind, timestamp: new Date().toISOString().slice(0, 19) + 'Z',
    source: 'nell', session: deviceId(), channel, about, private: true,
  };
  entries.push(entry);
  while (entries.length > CAP) entries.shift();
  try { s.setItem(KEY, JSON.stringify(entries)); } catch { return null; }
  return entry;
}
export function count() { return all().length; }
export function newest(n = 5) { return all().slice(-n).reverse(); }
export function exportJson() { return JSON.stringify(all(), null, 1); }
export function clear() { const s = storage(); if (s) s.removeItem(KEY); }
