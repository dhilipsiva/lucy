// N3b's test: thresholds decide, in both directions; people are only ever Person.
import { Senses, parseEventLine, THRESHOLDS } from '../senses.js';
const changes = [];
const s = new Senses({ onChange: (c) => changes.push(c) });
let failed = false;
const t0 = 1_000_000;
s.ingest(parseEventLine(JSON.stringify({ t: t0, detections: [{ class: 'bird', confidence: 0.9 }] })));
s.ingest(parseEventLine(JSON.stringify({ t: t0 + 500, detections: [{ class: 'bird', confidence: 0.3 }] }))); // too unsure: no hit
s.ingest(parseEventLine(JSON.stringify({ t: t0 + 1000, detections: [{ class: 'bird', confidence: 0.8 }] })));
if (changes.length !== 0) { console.error('appeared too early'); failed = true; }
s.ingest(parseEventLine(JSON.stringify({ t: t0 + 1500, detections: [{ class: 'bird', confidence: 0.7 }, { class: 'person', confidence: 0.9 }] })));
if (changes.length !== 1 || changes[0].cls !== 'Bird' || changes[0].present !== true) { console.error('a bird should have appeared after three sure sightings'); failed = true; }
s.tick(t0 + 1500 + THRESHOLDS.vanishMs - 1);
if (changes.length !== 1) { console.error('vanished too early'); failed = true; }
s.tick(t0 + 1500 + THRESHOLDS.vanishMs + 1);
if (changes.length !== 2 || changes[1].present !== false) { console.error('the bird should be gone after the vanish time'); failed = true; }
// a person needs three hits too, and is only ever Person
s.simulate('Person', true, t0 + 9000);
if (!changes.some((c) => c.cls === 'Person' && c.present)) { console.error('simulate(Person) should appear'); failed = true; }
if (parseEventLine('not json') !== null) { console.error('bad line should be null'); failed = true; }
if (parseEventLine('{"class":"unicorn","confidence":1}').detections.length !== 0) { console.error('unknown classes are dropped'); failed = true; }
console.log(JSON.stringify({ changes: changes.map((c) => `${c.cls}:${c.present ? 'here' : 'gone'}`), thresholds: THRESHOLDS }));
process.exit(failed ? 1 : 0);
