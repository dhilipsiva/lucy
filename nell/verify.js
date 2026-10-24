// Pinned remote code and bytes (vendor/REMOTE): fetched, hashed, compared with the pin, and
// only then imported or used. A hash that does not match stops the load; the caller says so.
export async function sha256Hex(buf) {
  const d = await crypto.subtle.digest('SHA-256', buf);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}
export async function verifiedBytes({ url, sha256 }) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status} ${res.statusText}`);
  const buf = await res.arrayBuffer();
  const got = await sha256Hex(buf);
  if (got !== sha256) throw new Error(`${url}: sha256 ${got.slice(0, 12)}… is not the pinned ${sha256.slice(0, 12)}…; not loaded`);
  return buf;
}
/** Import a module only after the bytes at its address hashed to the pin (the import then reads the browser's cache of the same fetch). */
export async function verifiedImport(pin) {
  await verifiedBytes(pin);
  return import(/* @vite-ignore */ pin.url);
}
