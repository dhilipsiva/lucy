// Eyes, through his phone, as events (roadmap/N3b.md). The phone runs the detector
// and sends detections; this module turns them into STATE only when a threshold is
// crossed, and reports each crossing. Nothing here keeps a detection or an image.

export const THRESHOLDS = Object.freeze({
  appearHits: 3,        // sightings needed…
  appearWindowMs: 3000, // …within this window…
  minConfidence: 0.6,   // …each at least this sure
  vanishMs: 5000,       // gone after this long unseen
});
// Detector class names to the engine's constants. People are only ever Person.
export const CLASSES = Object.freeze({ bird: 'Bird', dog: 'Dog', cat: 'Cat', person: 'Person' });

/** One JSON line from a source into an event {t, detections:[{cls, confidence}]}, or null. */
export function parseEventLine(line) {
  let v;
  try { v = JSON.parse(String(line).trim()); } catch { return null; }
  if (!v || typeof v !== 'object') return null;
  const t = Number.isFinite(v.t) ? v.t : Date.now();
  const raw = Array.isArray(v.detections) ? v.detections : (v.class ? [v] : []);
  const detections = raw
    .map((d) => ({ cls: CLASSES[String(d.class || d.cls || '').toLowerCase()], confidence: Number(d.confidence ?? d.score ?? 0) }))
    .filter((d) => d.cls && Number.isFinite(d.confidence));
  return { t, detections };
}

export class Senses {
  constructor({ thresholds = THRESHOLDS, onChange = () => {} } = {}) {
    this.thresholds = thresholds; this.onChange = onChange;
    this.state = new Map(); // cls -> { present, hits: [t…], lastSeen }
  }
  present() { return [...this.state.entries()].filter(([, s]) => s.present).map(([cls]) => cls); }
  ingest(event) {
    if (!event) return;
    const { appearHits, appearWindowMs, minConfidence } = this.thresholds;
    const t = event.t;
    const seenNow = new Set();
    for (const d of event.detections) {
      if (d.confidence < minConfidence || seenNow.has(d.cls)) continue;
      seenNow.add(d.cls);
      const s = this.state.get(d.cls) || { present: false, hits: [], lastSeen: 0 };
      s.hits = s.hits.filter((h) => t - h <= appearWindowMs); s.hits.push(t); s.lastSeen = t;
      if (!s.present && s.hits.length >= appearHits) { s.present = true; s.hits = []; this.onChange({ cls: d.cls, present: true, at: t }); }
      this.state.set(d.cls, s);
    }
    this.tick(t);
  }
  tick(now) {
    const { vanishMs } = this.thresholds;
    for (const [cls, s] of this.state) {
      if (s.present && now - s.lastSeen > vanishMs) { s.present = false; s.hits = []; this.onChange({ cls, present: false, at: now }); }
    }
  }
  /** The simulator: a thing appears (enough sightings at once) or leaves (unseen since long ago). */
  simulate(cls, present, now = Date.now()) {
    if (present) {
      for (let i = 0; i < this.thresholds.appearHits; i += 1) this.ingest({ t: now - (this.thresholds.appearHits - 1 - i) * 100, detections: [{ cls, confidence: 1 }] });
    } else {
      const s = this.state.get(cls); if (s) { s.lastSeen = now - this.thresholds.vanishMs - 1; s.hits = []; }
      this.tick(now);
    }
  }
}

// ── Bluetooth: the Nordic UART service, JSON lines in notifications ──────────
export const NUS = Object.freeze({
  service: '6e400001-b5a3-f393-e0a9-e50e24dcca9e',
  tx: '6e400003-b5a3-f393-e0a9-e50e24dcca9e', // the phone notifies, we read
});
export async function connectBluetooth({ onLine, log = () => {} }) {
  if (!navigator.bluetooth) throw new Error('this browser has no Web Bluetooth');
  const device = await navigator.bluetooth.requestDevice({ filters: [{ services: [NUS.service] }] });
  log(`eyes: pairing with ${device.name || 'an unnamed device'}`);
  const server = await device.gatt.connect();
  const service = await server.getPrimaryService(NUS.service);
  const tx = await service.getCharacteristic(NUS.tx);
  const decoder = new TextDecoder(); let buffer = '';
  tx.addEventListener('characteristicvaluechanged', (ev) => {
    buffer += decoder.decode(ev.target.value, { stream: true });
    let i;
    while ((i = buffer.indexOf('\n')) >= 0) { const line = buffer.slice(0, i); buffer = buffer.slice(i + 1); if (line.trim()) onLine(line); }
  });
  await tx.startNotifications();
  log(`eyes: watching through ${device.name || 'the phone'}`);
  return { name: device.name || 'the phone', disconnect: () => { try { device.gatt.disconnect(); } catch { /* already gone */ } }, device };
}
