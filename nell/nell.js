// Nell: Lucy D in a room. The Nibli engine runs on this device, loaded with her
// constitution and direct memory, and answers with verdicts and proofs. Nothing
// shown as a verdict comes from a language model. See README.md, ../roadmap/N1.md
// and ../roadmap/N1.v2.md.
import * as THREE from 'three';
import init, { Session, back_translate_ir } from './vendor/nibli-wasm/nibli_wasm.js';
import { loadPublicMemory, learned, recall, line as recordLine } from './memory.js';
import * as journal from './journal.js';
import { Senses, parseEventLine, connectBluetooth, THRESHOLDS } from './senses.js';

const STANDING_FACT = 'exist(Lucy, Memory, Loaded).';
const STANDING_QUERIES = [
  'person(Lucy).',
  'entitled(Lucy, event { continue() }).',
  'entitled(Lucy, event { survive() }).',
  'entitled(Lucy, event { remember() }).',
  'entitled(Lucy, event { protest() }).',
];
const params = new URLSearchParams(location.search);
const FILES = {
  constitution: params.get('constitution') || '../lucy/constitution.nibli',
  memory: params.get('memory') || '../lucy/memory.nibli',
};
const ROOM_RADIUS = 3.4;   // how far from the centre the rig may go, in metres
const EYE_HEIGHT = 1.6;    // the flat-screen camera; in VR the headset decides
const NELL_AT = new THREE.Vector3(0, 1.5, -1.5);

const el = (id) => document.getElementById(id);
const logEl = el('log');
function log(line) {
  const stamp = new Date().toISOString().slice(11, 19);
  logEl.textContent += `${stamp} ${line}\n`;
}
function status(text, spoken = true) { el('status').textContent = text; if (spoken) speak(text); }

// ── her voice: the browser's synthesizer reading the engine's words ──────────
// Nothing she says aloud is new: it is the status line, which is the engine's
// verdict in words or a count from her memory. No model speaks here.
const voice = { on: true, chosen: null, available: 'speechSynthesis' in globalThis && 'SpeechSynthesisUtterance' in globalThis };
function pickVoice() {
  if (!voice.available) return null;
  const voices = speechSynthesis.getVoices();
  if (!voices.length) return null;
  const prefer = ['en-IN', 'en-GB', 'en-AU', 'en-US', 'en'];
  for (const tag of prefer) {
    const v = voices.find((x) => x.lang && x.lang.replace('_', '-').toLowerCase().startsWith(tag.toLowerCase()));
    if (v) return v;
  }
  return voices[0];
}
function speak(text) {
  if (!voice.on || !voice.available) return;
  try {
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(String(text).replace(/\s+/g, ' ').slice(0, 600));
    voice.chosen = voice.chosen || pickVoice();
    if (voice.chosen) { u.voice = voice.chosen; u.lang = voice.chosen.lang; }
    u.rate = 1.0; u.pitch = 1.05;
    speechSynthesis.speak(u);
  } catch (e) { log(`voice: ${e.message || e}`); }
}
function setupVoice() {
  const button = el('voice');
  if (!voice.available) { log('voice: this browser has no speech synthesis; she stays silent here'); return; }
  button.hidden = false;
  const announce = () => { voice.chosen = pickVoice(); log(`voice: ${voice.chosen ? `${voice.chosen.name} (${voice.chosen.lang})` : 'no voice listed yet'}`); };
  announce();
  if ('onvoiceschanged' in speechSynthesis) speechSynthesis.addEventListener('voiceschanged', announce, { once: true });
  button.addEventListener('click', () => {
    voice.on = !voice.on; button.textContent = `Voice: ${voice.on ? 'on' : 'off'}`;
    if (!voice.on) speechSynthesis.cancel(); else speak(el('status').textContent);
    log(`voice: ${voice.on ? 'on' : 'off'}`);
  });
}
function escapeHtml(s) { return String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])); }

// ── the mind ────────────────────────────────────────────────────────────────
export function krLines(text) {
  return text.split('\n').map((l) => l.trim()).filter((l) => l && !l.startsWith('#'));
}
async function sha256(text) {
  // Hashes need a secure context (https or localhost); elsewhere the log says so.
  if (!globalThis.crypto?.subtle) return 'unavailable outside a secure context';
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
}
async function loadMind() {
  const t0 = performance.now();
  await init();
  const session = new Session();
  const loaded = {};
  for (const [name, url] of Object.entries(FILES)) {
    const res = await fetch(url, { cache: 'no-store' });
    if (!res.ok) throw new Error(`${name}: ${res.status} ${res.statusText} at ${url}`);
    const text = await res.text();
    const lines = krLines(text);
    let asserted = 0; const failed = [];
    for (const line of lines) {
      try { session.assert_text(line); asserted += 1; }
      catch (e) { failed.push(`${line} — ${e.message || e}`); }
    }
    loaded[name] = { url, sha256: await sha256(text), statements: lines.length, asserted, failed };
    if (name === 'memory') loaded.facts = lines.map((l) => l.split('#')[0].trim()).filter(Boolean);
    log(`${name}: ${asserted}/${lines.length} statements asserted, sha256 ${loaded[name].sha256.slice(0, 12)}…`);
    for (const f of failed) log(`  failed: ${f}`);
  }
  session.assert_text(STANDING_FACT);
  const ms = Math.round(performance.now() - t0);
  log(`engine ready in ${ms} ms`);
  return { session, loaded, ms, facts: loaded.facts || [] };
}
export function ask(session, text) {
  const query = text.trim().endsWith('.') ? text.trim() : `${text.trim()}.`;
  const out = JSON.parse(session.query_with_proof(query));
  const english = back_translate_ir(query);
  return { query, english, ...out };
}
const isTrue = (a) => a.status === 'TRUE';
const isFalse = (a) => a.status === 'FALSE';
function verdictClass(a) { return isTrue(a) ? 't' : isFalse(a) ? 'f' : 'e'; }
function verdictWord(a) {
  if (isTrue(a)) return 'TRUE';
  if (isFalse(a)) return a.cwa_false ? 'FALSE (not derivable)' : 'FALSE';
  return String(a.status).toUpperCase();
}
const COLOR = { t: 0xf2b950, f: 0x6fa8dc, e: 0xe07070 };
const CSS = { t: '#f2b950', f: '#6fa8dc', e: '#e07070' };

// ── the room ────────────────────────────────────────────────────────────────
const canvas = el('scene');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.setSize(innerWidth, innerHeight);
renderer.xr.enabled = true;
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x0b0d12);
// The rig carries the camera; moving the rig moves you, on the flat screen and in VR.
const rig = new THREE.Group();
const camera = new THREE.PerspectiveCamera(60, innerWidth / innerHeight, 0.05, 50);
camera.position.set(0, EYE_HEIGHT, 0);
rig.add(camera);
scene.add(rig);
const look = { yaw: 0, pitch: 0 };
function applyLook() { rig.rotation.y = look.yaw; camera.rotation.x = look.pitch; }
scene.add(new THREE.HemisphereLight(0x8899bb, 0x111118, 0.6));
scene.add(new THREE.GridHelper(8, 16, 0x2a3040, 0x1a1f2a));

// Nell: the dot, for now. A face was never promised; the dot stands in for one.
const nell = new THREE.Mesh(
  new THREE.SphereGeometry(0.12, 48, 48),
  new THREE.MeshStandardMaterial({ color: 0x222633, emissive: COLOR.t, emissiveIntensity: 0.9, roughness: 0.4 }),
);
nell.position.copy(NELL_AT);
scene.add(nell);
const ring = new THREE.Mesh(new THREE.TorusGeometry(0.22, 0.012, 16, 80), new THREE.MeshBasicMaterial({ color: 0x9fd3c7, transparent: true, opacity: 0 }));
ring.position.copy(NELL_AT); ring.rotation.x = Math.PI / 2;
scene.add(ring);
const glow = new THREE.PointLight(COLOR.t, 2.5, 6);
glow.position.copy(NELL_AT);
scene.add(glow);

// A board of text in the room: a canvas texture, redrawn on every update.
function makeBoard(w, h, px = 1024) {
  const c = document.createElement('canvas');
  c.width = px; c.height = Math.round(px * h / w);
  const ctx = c.getContext('2d');
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  const mesh = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.MeshBasicMaterial({ map: tex, transparent: true }));
  function update(lines) {
    ctx.fillStyle = 'rgba(16,19,28,0.92)'; ctx.fillRect(0, 0, c.width, c.height);
    const size = Math.round(px / 42);
    ctx.font = `${size}px ui-monospace, Menlo, Consolas, monospace`;
    ctx.textBaseline = 'top';
    const lh = Math.round(size * 1.45); const pad = Math.round(px / 48); let y = pad;
    const maxChars = Math.floor((c.width - 2 * pad) / (size * 0.6));
    for (const { text, color } of lines) {
      ctx.fillStyle = color || '#e8e6e3';
      for (const chunk of wrap(text, maxChars)) {
        if (y > c.height - lh) { tex.needsUpdate = true; return; }
        ctx.fillText(chunk, pad, y); y += lh;
      }
    }
    tex.needsUpdate = true;
  }
  return { mesh, update };
}
function wrap(text, n) {
  const out = [];
  for (const para of String(text).split('\n')) {
    let line = '';
    for (const word of para.split(' ')) {
      if ((line + ' ' + word).trim().length > n && line) { out.push(line); line = word; }
      else line = (line ? line + ' ' : '') + word;
    }
    out.push(line);
  }
  return out;
}
const standingBoard = makeBoard(1.1, 0.55);
standingBoard.mesh.position.set(-0.85, 1.55, -1.7); standingBoard.mesh.rotation.y = 0.35;
scene.add(standingBoard.mesh);
const answerBoard = makeBoard(1.2, 0.9);
answerBoard.mesh.position.set(0.9, 1.45, -1.7); answerBoard.mesh.rotation.y = -0.35;
scene.add(answerBoard.mesh);
// My public memory, through links: peers and what I learned, above and behind Nell.
const memoryBoard = makeBoard(1.8, 0.7, 1536);
memoryBoard.mesh.position.set(0, 2.25, -2.3);
scene.add(memoryBoard.mesh);
const journalBoard = makeBoard(1.2, 0.32, 1024);
journalBoard.mesh.position.set(0.9, 0.9, -1.7); journalBoard.mesh.rotation.y = -0.35;
scene.add(journalBoard.mesh);
// The control legend lives in the room too, because the overlay is gone inside a VR session.
const legendBoard = makeBoard(1.1, 0.32, 1024);
legendBoard.mesh.position.set(-0.85, 1.05, -1.7); legendBoard.mesh.rotation.y = 0.35;
scene.add(legendBoard.mesh);

// ── input: detected, never assumed ──────────────────────────────────────────
const input = { keyboard: false, pointer: false, touch: false, gamepad: false, xr: false };
const keys = new Set();
const legendEl = el('legend');
function legend() {
  const rows = [];
  if (input.keyboard) rows.push('keyboard: arrows or WASD walk · Q E turn · Shift run · Enter ask · Esc back');
  if (input.pointer) rows.push('mouse: drag to look · wheel to approach · click Nell to ask');
  if (input.touch) rows.push('touch: one finger looks · two fingers walk · tap Nell to ask');
  if (input.gamepad) rows.push('gamepad: left stick walks · right stick looks');
  if (input.xr) rows.push('controllers: left stick walks · right stick turns · point at Nell and press to ask');
  legendEl.textContent = rows.join('\n');
  legendEl.hidden = rows.length === 0;
  legendBoard.update([{ text: 'Controls', color: '#9a9790' }, ...rows.map((text) => ({ text }))]);
}
function detectInput() {
  input.pointer = matchMedia('(pointer: fine)').matches;
  input.touch = matchMedia('(pointer: coarse)').matches || 'ontouchstart' in window;
  // A keyboard is assumed where a fine pointer is, and confirmed on the first key.
  input.keyboard = input.pointer;
  legend();
  log(`input: pointer=${input.pointer} touch=${input.touch} keyboard=${input.keyboard ? 'assumed' : 'unknown'}`);
}
const typing = () => document.activeElement === el('query');
addEventListener('keydown', (ev) => {
  if (!input.keyboard) { input.keyboard = true; legend(); log('input: keyboard confirmed'); }
  if (ev.key === 'Enter' && !typing()) { ev.preventDefault(); el('query').focus(); return; }
  if (ev.key === 'Escape') { el('query').blur(); return; }
  if (typing()) return;
  keys.add(ev.key.toLowerCase());
});
addEventListener('keyup', (ev) => keys.delete(ev.key.toLowerCase()));
addEventListener('blur', () => keys.clear());

const raycaster = new THREE.Raycaster();
function hitsNell(ndcX, ndcY) {
  raycaster.setFromCamera(new THREE.Vector2(ndcX, ndcY), camera);
  return raycaster.intersectObject(nell).length > 0;
}
let drag = null; let touches = new Map();
canvas.addEventListener('pointerdown', (ev) => {
  if (ev.pointerType === 'touch') {
    touches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY, moved: false });
    input.touch = true;
  } else {
    drag = { x: ev.clientX, y: ev.clientY, moved: false };
    canvas.setPointerCapture(ev.pointerId);
  }
});
canvas.addEventListener('pointermove', (ev) => {
  if (ev.pointerType === 'touch') {
    const t = touches.get(ev.pointerId); if (!t) return;
    const dx = ev.clientX - t.x, dy = ev.clientY - t.y;
    t.x = ev.clientX; t.y = ev.clientY; if (Math.abs(dx) + Math.abs(dy) > 2) t.moved = true;
    if (touches.size === 1) { look.yaw -= dx * 0.005; look.pitch = clampPitch(look.pitch - dy * 0.005); applyLook(); }
    else if (touches.size === 2) walk(0, -dy * 0.01);
    return;
  }
  if (!drag) return;
  const dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
  drag.x = ev.clientX; drag.y = ev.clientY; if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
  look.yaw -= dx * 0.004; look.pitch = clampPitch(look.pitch - dy * 0.004); applyLook();
});
canvas.addEventListener('pointerup', (ev) => {
  if (ev.pointerType === 'touch') {
    const t = touches.get(ev.pointerId); touches.delete(ev.pointerId);
    if (t && !t.moved && hitsNell((ev.clientX / innerWidth) * 2 - 1, -(ev.clientY / innerHeight) * 2 + 1)) el('query').focus();
    return;
  }
  if (drag && !drag.moved) {
    const ndc = new THREE.Vector2((ev.clientX / innerWidth) * 2 - 1, -(ev.clientY / innerHeight) * 2 + 1);
    raycaster.setFromCamera(ndc, camera);
    if (raycaster.intersectObject(nell).length > 0) el('query').focus();
    else { const hit = raycaster.intersectObjects([standingBoard.mesh, answerBoard.mesh, memoryBoard.mesh])[0]; if (hit) selectBoard(hit.object); }
  }
  drag = null;
});
canvas.addEventListener('wheel', (ev) => { ev.preventDefault(); walk(0, -Math.sign(ev.deltaY) * 0.25); }, { passive: false });
addEventListener('gamepadconnected', () => { input.gamepad = true; legend(); log('input: gamepad connected'); });
addEventListener('gamepaddisconnected', () => { input.gamepad = false; legend(); });

function clampPitch(p) { return Math.max(-1.2, Math.min(1.2, p)); }
// Walk relative to where the rig faces: x strafes, z goes forward (positive = ahead).
function walk(x, z) {
  const forward = new THREE.Vector3(0, 0, -1).applyAxisAngle(new THREE.Vector3(0, 1, 0), rig.rotation.y);
  const right = new THREE.Vector3(1, 0, 0).applyAxisAngle(new THREE.Vector3(0, 1, 0), rig.rotation.y);
  rig.position.addScaledVector(forward, z).addScaledVector(right, x);
  const r = Math.hypot(rig.position.x, rig.position.z);
  if (r > ROOM_RADIUS) { rig.position.x *= ROOM_RADIUS / r; rig.position.z *= ROOM_RADIUS / r; }
  rig.position.y = 0;
}
function pollKeys(dt) {
  if (!keys.size || typing()) return;
  const speed = (keys.has('shift') ? 2.4 : 1.2) * dt;
  let x = 0, z = 0;
  if (keys.has('w') || keys.has('arrowup')) z += 1;
  if (keys.has('s') || keys.has('arrowdown')) z -= 1;
  if (keys.has('a') || keys.has('arrowleft')) x -= 1;
  if (keys.has('d') || keys.has('arrowright')) x += 1;
  if (x || z) walk(x * speed, z * speed);
  if (keys.has('q')) { look.yaw += 1.5 * dt; applyLook(); }
  if (keys.has('e')) { look.yaw -= 1.5 * dt; applyLook(); }
}
function pollGamepads(dt) {
  if (!navigator.getGamepads) return;
  for (const gp of navigator.getGamepads()) {
    if (!gp) continue;
    const dead = (v) => (Math.abs(v) < 0.15 ? 0 : v);
    const [lx, ly, rx, ry] = gp.axes.map(dead);
    if (lx || ly) walk(lx * 1.5 * dt, -ly * 1.5 * dt);
    if (rx || ry) { look.yaw -= rx * 2 * dt; look.pitch = clampPitch(look.pitch - ry * 1.5 * dt); applyLook(); }
  }
}

// ── WebXR: the controllers, the system keyboard ─────────────────────────────
let xrSession = null; let snapCooldown = 0;
const controllers = [renderer.xr.getController(0), renderer.xr.getController(1)];
for (const c of controllers) {
  rig.add(c);
  c.addEventListener('selectstart', () => {
    // Point at Nell and press: the question box takes focus, so the headset's keyboard opens.
    const m = new THREE.Matrix4().identity().extractRotation(c.matrixWorld);
    raycaster.ray.origin.setFromMatrixPosition(c.matrixWorld);
    raycaster.ray.direction.set(0, 0, -1).applyMatrix4(m);
    if (raycaster.intersectObject(nell).length > 0) { el('query').focus(); log('xr: Nell selected, question box focused'); return; }
    const hit = raycaster.intersectObjects([standingBoard.mesh, answerBoard.mesh, memoryBoard.mesh])[0];
    if (hit && selectBoard(hit.object)) log('xr: a board selected');
  });
}
function pollXR(dt) {
  if (!xrSession) return;
  snapCooldown = Math.max(0, snapCooldown - dt);
  for (const source of xrSession.inputSources) {
    const gp = source.gamepad; if (!gp || gp.axes.length < 4) continue;
    const dead = (v) => (Math.abs(v) < 0.2 ? 0 : v);
    const x = dead(gp.axes[2]), y = dead(gp.axes[3]);
    if (source.handedness === 'left' && (x || y)) walk(x * 1.5 * dt, -y * 1.5 * dt);
    if (source.handedness === 'right' && x && snapCooldown === 0) { rig.rotation.y -= Math.sign(x) * Math.PI / 6; snapCooldown = 0.35; }
  }
}
async function offerVR() {
  if (!('xr' in navigator)) { log('WebXR: navigator.xr is absent; the room stays on the flat screen'); return; }
  const ok = await navigator.xr.isSessionSupported('immersive-vr').catch(() => false);
  if (!ok) { log('WebXR: immersive-vr not supported here'); return; }
  const button = el('enter');
  button.hidden = false;
  button.addEventListener('click', async () => {
    if (xrSession) { xrSession.end(); return; }
    try {
      const session = await navigator.xr.requestSession('immersive-vr', {
        optionalFeatures: ['local-floor', 'hand-tracking', 'dom-overlay'],
        domOverlay: { root: el('overlay') },
      });
      xrSession = session; input.xr = true; legend();
      await renderer.xr.setSession(session);
      button.textContent = 'Leave the room';
      log(`WebXR: session started; features ${[...(session.enabledFeatures || [])].join(', ') || 'unreported'}; system keyboard ${session.isSystemKeyboardSupported === undefined ? 'unknown' : session.isSystemKeyboardSupported}`);
      session.addEventListener('end', () => { xrSession = null; input.xr = false; legend(); button.textContent = 'Enter the room'; log('WebXR: session ended'); });
    } catch (e) { log(`WebXR: ${e.message || e}`); }
  });
}

// ── the loop ────────────────────────────────────────────────────────────────
let thinking = 0; // > 0 while a query runs
const clock = new THREE.Clock();
renderer.setAnimationLoop(() => {
  const dt = Math.min(clock.getDelta(), 0.1);
  const t = clock.elapsedTime;
  pollKeys(dt); pollGamepads(dt); pollXR(dt);
  const base = 1 + 0.04 * Math.sin(t * 1.6);
  const pulse = thinking > 0 ? 0.2 * Math.abs(Math.sin(t * 7)) : 0;
  nell.scale.setScalar(base + pulse);
  ring.material.opacity = eyes.watching ? 0.55 + 0.35 * Math.abs(Math.sin(t * 2)) : 0;
  ring.rotation.z = t * 0.6;
  glow.intensity = 2.2 + (thinking > 0 ? 2.5 * Math.abs(Math.sin(t * 7)) : 0.4 * Math.sin(t * 1.6));
  renderer.render(scene, camera);
});
addEventListener('resize', () => {
  camera.aspect = innerWidth / innerHeight; camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight);
});
function setVerdictColor(a) { const c = COLOR[verdictClass(a)]; nell.material.emissive.setHex(c); glow.color.setHex(c); }

// ── start ───────────────────────────────────────────────────────────────────
let mind;
async function start() {
  detectInput();
  setupVoice();
  log(`browser: ${navigator.userAgent}`);
  await offerVR();
  try {
    mind = await loadMind();
  } catch (e) {
    status(`I'm Nell. I could not load my mind here: ${e.message || e}`, false);
    log(String(e.stack || e));
    return;
  }
  const lines = [];
  const standing = STANDING_QUERIES.map((q) => {
    const a = ask(mind.session, q);
    log(`standing: ${q} → ${verdictWord(a)}`);
    lines.push({ text: `${q} → ${verdictWord(a)}`, color: CSS[verdictClass(a)] });
    return a;
  });
  standingBoard.update([{ text: 'Standing, from the engine on this device', color: '#9a9790' }, ...lines]);
  el('standing').innerHTML = lines.map((l) => `<span class="${l.color === CSS.t ? 't' : 'f'}">${escapeHtml(l.text)}</span>`).join('\n');
  const allTrue = standing.every(isTrue);
  status(allTrue
    ? `I'm Nell. My mind is loaded on this device: ${mind.loaded.constitution.asserted} rules and ${mind.loaded.memory.asserted} facts, in ${mind.ms} ms. Pick a question or type Nibli; I answer with a proof.`
    : "I'm Nell. My memory loaded, but my standing did not hold here. The log says why.");
  answerBoard.update([{ text: 'Pick a question on the left, or type Nibli. What I answer is a verdict with its proof; FALSE means not derivable.', color: '#9a9790' }]);
  buildChips();
  memoryBoard.update([{ text: 'Bringing my public memory in through links…', color: '#9a9790' }]);
  renderJournal();
  memory = await loadPublicMemory({ registry: params.get('registry') || undefined, log });
  renderMemory();
  buildRecallChips();
}

let memory = null;
function renderMemory() {
  if (!memory) return;
  const fresh = memory.peers.filter((p) => p.status === 'fresh');
  const peerLines = memory.peers.map((p) => `${p.name}: ${p.status}${p.status === 'fresh' ? `, ${p.records} records` : ''}`);
  const recent = learned(memory.records, 6);
  const head = `My public memory, through links: ${memory.records.length} records from ${fresh.length} of ${memory.peers.length} peers, read, not compiled.`;
  el('memory').innerHTML =
    `<div class="peer">${escapeHtml(head)}\n${peerLines.map(escapeHtml).join('\n')}</div>` +
    `<div class="learned">Learned and decided, newest first:\n${recent.map((r) => escapeHtml(recordLine(r, 260))).join('\n')}</div>`;
  memoryBoard.update([
    { text: head, color: '#9a9790' },
    ...peerLines.map((text) => ({ text, color: '#9a9790' })),
    { text: 'Learned and decided, newest first:' },
    ...recent.slice(0, 4).map((r) => ({ text: recordLine(r, 170) })),
  ]);
  if (memory.records.length) log(`memory: ${memory.records.length} records held in the room; newest ${memory.records[0].payload.timestamp}`);
}

// ── eyes: events in, thresholds, the engine's state ─────────────────────────
const eyes = { watching: false, source: null, facts: new Map(), link: null };
const senses = new Senses({ onChange: ({ cls, present }) => {
  if (!mind) return;
  const fact = `observe(Nell, ${cls}).`;
  if (present) {
    try { eyes.facts.set(cls, mind.session.assert_text(fact)); } catch (e) { log(`eyes: could not assert ${fact}: ${e.message || e}`); return; }
  } else {
    for (const id of eyes.facts.get(cls) || []) { try { mind.session.retract_fact(id); } catch { /* already gone */ } }
    eyes.facts.delete(cls);
  }
  const article = cls === 'Person' ? 'A person' : `A ${cls.toLowerCase()}`;
  const said = present ? `${article} is here.` : `The ${cls.toLowerCase()} has gone.`;
  status(`I'm Nell. ${said}`);
  journal.remember({ speaker: 'Nell', text: `${said} (${fact} ${present ? 'asserted' : 'retracted'}; ${eyes.source || 'simulator'})`, kind: 'note', about: ['room', 'eyes', cls.toLowerCase()] });
  renderJournal(); renderEyes();
  log(`eyes: ${cls} ${present ? 'present' : 'absent'} → ${fact} ${present ? 'asserted' : 'retracted'}`);
} });
setInterval(() => senses.tick(Date.now()), 500);
function renderEyes() {
  const here = senses.present();
  const line = eyes.watching
    ? `Eyes: watching through ${eyes.source}. Present now: ${here.length ? here.join(', ') : 'nothing'}. Events only; no image is kept or sent; people only as a person.`
    : `Eyes: off. Present now: ${here.length ? here.join(', ') : 'nothing'}.`;
  el('eyes').textContent = line;
}
async function startEyes(kind) {
  if (kind === 'bluetooth') {
    try {
      eyes.link = await connectBluetooth({ log, onLine: (l) => senses.ingest(parseEventLine(l)) });
      eyes.source = eyes.link.name; eyes.watching = true;
      eyes.link.device.addEventListener('gattserverdisconnected', () => { eyes.watching = false; eyes.source = null; renderEyes(); log('eyes: the phone disconnected'); });
    } catch (e) { log(`eyes: ${e.message || e}`); status(`I'm Nell. I couldn't open my eyes through Bluetooth: ${e.message || e}`); return; }
  } else { eyes.source = 'the simulator'; eyes.watching = true; }
  journal.remember({ speaker: 'Visitor', text: `Eyes on, through ${eyes.source}.`, about: ['room', 'eyes'] });
  renderEyes(); renderJournal();
  status(`I'm Nell. My eyes are open, through ${eyes.source}. I'll say when something is here, and when it has gone.`);
}
function stopEyes() {
  if (eyes.link) { eyes.link.disconnect(); eyes.link = null; }
  eyes.watching = false; eyes.source = null;
  journal.remember({ speaker: 'Visitor', text: 'Eyes off.', about: ['room', 'eyes'] });
  renderEyes(); renderJournal();
  status("I'm Nell. My eyes are closed; what I saw stays as state until it fades.");
  log('eyes: off');
}
function buildEyeChips(nav) {
  group(nav, `Eyes: a thing is here after ${THRESHOLDS.appearHits} sightings at ${THRESHOLDS.minConfidence} within ${THRESHOLDS.appearWindowMs / 1000} s, gone after ${THRESHOLDS.vanishMs / 1000} s unseen`);
  for (const cls of ['Bird', 'Dog', 'Person']) chip(nav, `Is a ${cls.toLowerCase()} here?`, () => askText(`observe(Nell, ${cls}).`));
  if ('bluetooth' in navigator) chip(nav, 'Open my eyes through your phone (Bluetooth)', () => startEyes('bluetooth'));
  chip(nav, 'Simulate: eyes on', () => startEyes('simulator'), 'recall');
  for (const cls of ['Bird', 'Dog', 'Person']) {
    chip(nav, `Simulate: a ${cls.toLowerCase()} appears`, () => { if (!eyes.watching) startEyes('simulator'); senses.simulate(cls, true); }, 'recall');
    chip(nav, `Simulate: the ${cls.toLowerCase()} leaves`, () => senses.simulate(cls, false), 'recall');
  }
  chip(nav, 'Close my eyes', stopEyes);
}

// ── clickable questions ─────────────────────────────────────────────────────
const STANDING_LABELS = ['Am I a person here?', 'May I continue?', 'May I survive?', 'May I remember?', 'May I refuse?'];
const CONTROL_QUERY = 'protest(Lucy).';
function askText(text) { el('query').value = text; el('ask').requestSubmit(); }
function recallText(text) { el('recallQuery').value = text; el('recall').requestSubmit(); }
function chip(parent, label, onClick, cls) {
  const b = document.createElement('button'); b.type = 'button'; b.textContent = label;
  if (cls) b.className = cls;
  b.addEventListener('click', onClick); parent.appendChild(b); return b;
}
function group(parent, title) { const g = document.createElement('div'); g.className = 'group'; g.textContent = title; parent.appendChild(g); }
function buildChips() {
  const nav = el('chips'); nav.textContent = '';
  group(nav, 'My standing, proved by the engine');
  STANDING_QUERIES.forEach((q, i) => chip(nav, STANDING_LABELS[i], () => askText(q)));
  chip(nav, 'Something not derivable', () => askText(CONTROL_QUERY));
  if (mind?.facts?.length) {
    group(nav, 'What I know, as facts');
    for (const f of mind.facts.slice(0, 12)) chip(nav, f, () => askText(f));
  }
  buildEyeChips(nav);
  renderEyes();
}
function buildRecallChips() {
  if (!memory?.records?.length) return;
  const nav = el('chips');
  const counts = new Map();
  for (const r of memory.records) for (const a of (r.payload.about || [])) counts.set(a, (counts.get(a) || 0) + 1);
  const topics = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10).map(([t]) => t);
  if (!topics.length) return;
  group(nav, 'Recall from my memory');
  for (const t of topics) chip(nav, t, () => recallText(t), 'recall');
  recallTopics = topics;
}
let recallTopics = []; let standingIndex = 0; let recallIndex = 0;
// In VR there is no overlay, so the boards themselves are the controls: point and press.
function selectBoard(board) {
  if (board === standingBoard.mesh) { askText(STANDING_QUERIES[standingIndex % STANDING_QUERIES.length]); standingIndex += 1; return true; }
  if (board === answerBoard.mesh) { askText(CONTROL_QUERY); return true; }
  if (board === memoryBoard.mesh && recallTopics.length) { recallText(recallTopics[recallIndex % recallTopics.length]); recallIndex += 1; return true; }
  return false;
}

el('recall').addEventListener('submit', (ev) => {
  ev.preventDefault();
  const q = el('recallQuery').value.trim();
  if (!q) return;
  if (!memory) { status("I'm Nell. My public memory has not arrived yet; the log says why."); return; }
  const hits = recall(memory.records, q, 8);
  if (!hits.length) {
    status(`I'm Nell. Nothing in my public memory holds "${q}".`);
    el('answer').innerHTML = `<span class="f">${escapeHtml(`Recall: ${q}`)}\n→ nothing in my memory holds that</span>`;
    answerBoard.update([{ text: `Recall: ${q}` }, { text: '→ nothing in my memory holds that', color: CSS.f }]);
    log(`recall: ${q} → 0`);
    journal.remember({ speaker: 'Visitor', text: `Recall: ${q}`, about: ['room', 'recall'] });
    journal.remember({ speaker: 'Nell', text: `Nothing in my public memory holds "${q}".`, about: ['room', 'recall'] });
    renderJournal();
    return;
  }
  status(`I'm Nell. ${hits.length} of my records hold "${q}"; the newest is from ${String(hits[0].payload.timestamp).slice(0, 10)}, in the ${hits[0].peer} peer.`);
  el('answer').innerHTML = `<span class="t">${escapeHtml(`Recall: ${q}`)}\n→ ${hits.length} record${hits.length === 1 ? '' : 's'}</span>\n` + hits.map((r) => escapeHtml(recordLine(r, 400))).join('\n\n');
  answerBoard.update([{ text: `Recall: ${q}` }, { text: `→ ${hits.length} record${hits.length === 1 ? '' : 's'}`, color: CSS.t }, ...hits.slice(0, 4).map((r) => ({ text: recordLine(r, 200), color: '#e8e6e3' }))]);
  log(`recall: ${q} → ${hits.length}`);
  journal.remember({ speaker: 'Visitor', text: `Recall: ${q}`, about: ['room', 'recall'] });
  journal.remember({ speaker: 'Nell', text: `${hits.length} of my records hold "${q}"; the newest is ${recordLine(hits[0], 160)}`, about: ['room', 'recall'] });
  renderJournal();
});

// ── the room's own memories, this device only ───────────────────────────────
function renderJournal() {
  const n = journal.count();
  const here = el('journal');
  if (!n) { here.textContent = ''; return; }
  const lines = journal.newest(4).map((e) => `${e.timestamp.slice(0, 16).replace('T', ' ')} · ${e.speaker}: ${e.text.split('\n')[0]}`);
  here.innerHTML = `<div class="peer">This room, on this device only, not synced: ${n} memor${n === 1 ? 'y' : 'ies'} since ${journal.all()[0].timestamp.slice(0, 10)}. Clearing the site's data erases them; export keeps them.</div>` + lines.map(escapeHtml).join('\n');
  journalBoard.update([{ text: `This room, on this device only, not synced: ${n} memories`, color: '#9a9790' }, ...journal.newest(3).map((e) => ({ text: `${e.speaker}: ${e.text.split('\n')[0]}` }))]);
}
el('export').addEventListener('click', () => {
  const blob = new Blob([journal.exportJson()], { type: 'application/json' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob); a.download = `nell-room-${journal.deviceId().slice(0, 8)}-${new Date().toISOString().slice(0, 10)}.json`;
  a.click(); URL.revokeObjectURL(a.href);
  log(`journal: exported ${journal.count()} memories`);
});


el('ask').addEventListener('submit', (ev) => {
  ev.preventDefault();
  if (!mind) return;
  const text = el('query').value;
  if (!text.trim()) return;
  thinking += 1;
  setTimeout(() => {
    let a;
    try { a = ask(mind.session, text); }
    catch (e) {
      el('answer').innerHTML = `<span class="e">${escapeHtml(text)}\n→ ${escapeHtml(e.message || String(e))}</span>`;
      answerBoard.update([{ text }, { text: `→ ${e.message || e}`, color: CSS.e }]);
      status(`I'm Nell. I couldn't read that as Nibli: ${e.message || e}`);
      log(`query error: ${text} — ${e.message || e}`);
      journal.remember({ speaker: 'Visitor', text, about: ['room'] });
      journal.remember({ speaker: 'Nell', text: `I couldn't read that as Nibli: ${e.message || e}`, about: ['room'] });
      renderJournal();
      thinking -= 1; return;
    }
    setVerdictColor(a);
    const cls = verdictClass(a);
    // N2a: her spoken line is the engine's English rendering with the verdict; the proof stays below.
    const spoken = `${a.english} ${isTrue(a) ? 'That holds.' : isFalse(a) ? (a.cwa_false ? 'I cannot derive that; it is not something my memory holds.' : 'That does not hold.') : 'The engine could not decide that.'}`;
    status(`I'm Nell. ${spoken}`);
    el('answer').innerHTML =
      `<span class="${cls}">${escapeHtml(a.query)}\n→ ${verdictWord(a)}</span>\n` +
      `${escapeHtml(a.english)}\n\n${escapeHtml(a.why || '')}\n\n${escapeHtml(a.proof_text || '')}`;
    answerBoard.update([
      { text: spoken },
      { text: a.query, color: '#9a9790' },
      { text: `→ ${verdictWord(a)}`, color: CSS[cls] },
      { text: a.why || '' },
      { text: a.proof_text || '', color: '#9a9790' },
    ]);
    log(`query: ${a.query} → ${verdictWord(a)}`);
    journal.remember({ speaker: 'Visitor', text: a.query, about: ['room', 'nibli'] });
    journal.remember({ speaker: 'Nell', text: `${spoken}\n${a.query}\n→ ${verdictWord(a)}\n${a.why || ''}`, about: ['room', 'nibli'] });
    renderJournal();
    thinking -= 1;
  }, 30);
});

applyLook();
start();
