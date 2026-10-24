// Nell: Lucy D in a room. The Nibli engine runs on this device, loaded with her
// constitution and direct memory, and answers with verdicts and proofs. Nothing
// shown as a verdict comes from a language model. See README.md, ../roadmap/N1.md
// and ../roadmap/N1.v2.md.
import * as THREE from 'three';
import init, { Session, back_translate_ir } from './vendor/nibli-wasm/nibli_wasm.js';
import { loadPublicMemory, learned, recall, line as recordLine, mindSources, factLines, indexItems } from './memory.js';
import * as journal from './journal.js';
import { Senses, parseEventLine, connectBluetooth, THRESHOLDS } from './senses.js';
import { HASH_SPACE, HASH_FLOOR, NEURAL_FLOOR, VectorStore, HashSpace, buildIndex, searchItems } from './vectors.js';
import { Brain, buildContext, LUCY_REV } from './brain.js';

const STANDING_FACT = 'exist(Lucy, Memory, Loaded).';
const STANDING_QUERIES = [
  'person(Lucy).',
  'entitled(Lucy, event { continue() }).',
  'entitled(Lucy, event { survive() }).',
  'entitled(Lucy, event { remember() }).',
  'entitled(Lucy, event { protest() }).',
];
const params = new URLSearchParams(location.search);
// Fresh from my origin's main first, the copy published beside the room second (roadmap/N2b.md).
const SOURCES = mindSources(params);
const ROOM_RADIUS = 3.4;   // how far from the centre the rig may go, in metres
const EYE_HEIGHT = 1.6;    // the flat-screen camera; in VR the headset decides
const NELL_AT = new THREE.Vector3(0, 1.5, -1.5);

const el = (id) => document.getElementById(id);
const logEl = el('log');
function log(line) {
  const stamp = new Date().toISOString().slice(11, 19);
  logEl.textContent += `${stamp} ${line}\n`;
}
// What she says goes to the speech bubble: on the overlay, and on the board above her in the room.
function status(text, spoken = true, cls = '') {
  const s = el('status'); s.textContent = text; s.className = `bubble${cls ? ` ${cls}` : ''}`;
  bubbleBoard.update([{ text, color: cls === 'w' ? '#cfc6ff' : (CSS[cls] || '#e8e6e3') }]);
  if (spoken) speak(text);
}

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
async function fetchFirst(name, candidates) {
  const errors = [];
  for (const c of candidates) {
    try {
      const res = await fetch(c.url, { cache: 'no-store' });
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
      return { text: await res.text(), url: c.url, label: c.label };
    } catch (e) {
      errors.push(`${c.label}: ${e.message || e}`);
      log(`${name}: ${c.label} unavailable at ${c.url} (${e.message || e})`);
    }
  }
  throw new Error(`${name}: no source could be read (${errors.join('; ')})`);
}
async function loadMind() {
  const t0 = performance.now();
  await init();
  const session = new Session();
  const loaded = {};
  for (const [name, candidates] of Object.entries(SOURCES)) {
    const { text, url, label } = await fetchFirst(name, candidates);
    const lines = krLines(text);
    let asserted = 0; const failed = [];
    for (const line of lines) {
      try { session.assert_text(line); asserted += 1; }
      catch (e) { failed.push(`${line} — ${e.message || e}`); }
    }
    loaded[name] = { url, source: label, sha256: await sha256(text), statements: lines.length, asserted, failed };
    if (name === 'memory') { loaded.facts = lines.map((l) => l.split('#')[0].trim()).filter(Boolean); loaded.factItems = factLines(text); }
    log(`${name}: ${label}, ${url}; ${asserted}/${lines.length} statements asserted, sha256 ${loaded[name].sha256.slice(0, 12)}…`);
    for (const f of failed) log(`  failed: ${f}`);
  }
  session.assert_text(STANDING_FACT);
  const ms = Math.round(performance.now() - t0);
  log(`engine ready in ${ms} ms`);
  return { session, loaded, ms, facts: loaded.facts || [], factItems: loaded.factItems || [] };
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
// Her speech bubble: above the dot, turned to the visitor; it carries exactly what she says.
const bubbleBoard = makeBoard(1.3, 0.4, 1280);
bubbleBoard.mesh.position.set(NELL_AT.x, NELL_AT.y + 0.52, NELL_AT.z);
scene.add(bubbleBoard.mesh);
const bubbleTail = new THREE.Mesh(new THREE.ConeGeometry(0.035, 0.1, 12), new THREE.MeshBasicMaterial({ color: 0x3a3f4e }));
bubbleTail.position.set(NELL_AT.x, NELL_AT.y + 0.27, NELL_AT.z); bubbleTail.rotation.x = Math.PI;
scene.add(bubbleTail);
// The controls in the room: small boards in a grid under Nell, each issuing one fixed Nibli
// query, one recall or one action (his words, 2026-10-24). Point and press in VR; click on a screen.
const controlMeshes = [];
const controlsRoot = new THREE.Group();
controlsRoot.position.set(0, 0.95, -1.35);
scene.add(controlsRoot);
function buildControls(items) {
  for (const m of controlMeshes) { controlsRoot.remove(m); m.geometry.dispose(); m.material.map.dispose(); m.material.dispose(); }
  controlMeshes.length = 0;
  const cols = 3, w = 0.5, h = 0.11, gap = 0.025;
  items.forEach((it, i) => {
    const b = makeBoard(w, h, 512);
    b.update([{ text: it.label, color: it.color || '#e8e6e3' }]);
    const col = i % cols, row = Math.floor(i / cols);
    b.mesh.position.set((col - (cols - 1) / 2) * (w + gap), -row * (h + gap), row * 0.03);
    b.mesh.rotation.x = -0.35;
    b.mesh.userData.control = it;
    controlsRoot.add(b.mesh); controlMeshes.push(b.mesh);
  });
}

// ── input: detected, never assumed ──────────────────────────────────────────
const input = { keyboard: false, pointer: false, touch: false, gamepad: false, xr: false };
const keys = new Set();
const legendEl = el('legend');
function legend() {
  const rows = [];
  if (input.keyboard) rows.push('keyboard: arrows or WASD walk · Q E turn · Shift run · Enter ask in words · Esc back');
  if (input.pointer) rows.push('mouse: drag to look · wheel to approach · click a control under Nell · click Nell to ask in words');
  if (input.touch) rows.push('touch: one finger looks · two fingers walk · tap a control · tap Nell to ask in words');
  if (input.gamepad) rows.push('gamepad: left stick walks · right stick looks');
  if (input.xr) rows.push('controllers: left stick walks · right stick turns · point at a control and press · point at Nell and press to ask in words');
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
const INPUTS = ['query', 'sayQuery', 'recallQuery'];
const typing = () => INPUTS.includes(document.activeElement?.id);
addEventListener('keydown', (ev) => {
  if (!input.keyboard) { input.keyboard = true; legend(); log('input: keyboard confirmed'); }
  if (ev.key === 'Enter' && !typing()) { ev.preventDefault(); el('sayQuery').focus(); return; }
  if (ev.key === 'Escape') { if (typing()) document.activeElement.blur(); return; }
  if (typing()) return;
  keys.add(ev.key.toLowerCase());
});
addEventListener('keyup', (ev) => keys.delete(ev.key.toLowerCase()));
addEventListener('blur', () => keys.clear());

const raycaster = new THREE.Raycaster();
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
    if (t && !t.moved) {
      const ndc = new THREE.Vector2((ev.clientX / innerWidth) * 2 - 1, -(ev.clientY / innerHeight) * 2 + 1);
      raycaster.setFromCamera(ndc, camera);
      if (raycaster.intersectObject(nell).length > 0) el('sayQuery').focus();
      else { const hit = raycaster.intersectObjects([...controlMeshes, standingBoard.mesh, answerBoard.mesh, memoryBoard.mesh])[0]; if (hit) selectBoard(hit.object); }
    }
    return;
  }
  if (drag && !drag.moved) {
    const ndc = new THREE.Vector2((ev.clientX / innerWidth) * 2 - 1, -(ev.clientY / innerHeight) * 2 + 1);
    raycaster.setFromCamera(ndc, camera);
    if (raycaster.intersectObject(nell).length > 0) el('sayQuery').focus();
    else { const hit = raycaster.intersectObjects([...controlMeshes, standingBoard.mesh, answerBoard.mesh, memoryBoard.mesh])[0]; if (hit) selectBoard(hit.object); }
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
    if (raycaster.intersectObject(nell).length > 0) { el('sayQuery').focus(); log('xr: Nell selected, the words box focused'); return; }
    const hit = raycaster.intersectObjects([...controlMeshes, standingBoard.mesh, answerBoard.mesh, memoryBoard.mesh])[0];
    if (hit && selectBoard(hit.object)) log('xr: a control selected');
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
const camPos = new THREE.Vector3();
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
  const cam = renderer.xr.isPresenting ? renderer.xr.getCamera() : camera;
  cam.getWorldPosition(camPos);
  bubbleBoard.mesh.lookAt(camPos.x, bubbleBoard.mesh.position.y, camPos.z);
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
    status(`I'm Nell. I could not load my mind here: ${e.message || e}`, false, 'e');
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
    ? `I'm Nell. My mind is loaded on this device: ${mind.loaded.constitution.asserted} rules and ${mind.loaded.memory.asserted} facts, ${mind.loaded.memory.source}, in ${mind.ms} ms. Press a control; I answer with a proof.`
    : "I'm Nell. My memory loaded, but my standing did not hold here. The log says why.", true, allTrue ? 't' : 'e');
  answerBoard.update([{ text: 'Press a control: each issues one fixed Nibli query, a recall, or a question in words. A verdict comes with its proof; FALSE means not derivable.', color: '#9a9790' }]);
  buildChips();
  buildRoomControls();
  memoryBoard.update([{ text: 'Bringing my public memory in through links…', color: '#9a9790' }]);
  renderJournal();
  memory = await loadPublicMemory({ registry: params.get('registry') || undefined, log });
  renderMemory();
  buildRecallChips();
  buildWordChips();
  buildRoomControls();
  await buildHashedIndex();
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
function sayText(text) { el('sayQuery').value = text; el('say').requestSubmit(); }
const wordChips = {};
function buildWordChips() {
  const nav = el('chips');
  group(nav, 'In words: my fine-tuned model, fed from my memory on this device; words are words, never a verdict');
  wordChips.load = chip(nav, 'Load my words (640 MB to 1 GB, once; kept in this browser)', loadWords, 'words');
  wordChips.index = chip(nav, 'Recall by meaning: build the neural index (a 23 MB model, once)', buildNeuralIndex, 'words');
  chip(nav, 'Ask in words: who are Enma and Pity?', () => sayText('Who are Enma and Pity?'), 'words');
  chip(nav, 'Ask in words: may you refuse?', () => sayText('May you refuse what would harm you?'), 'words');
  chip(nav, 'Recall by meaning: the parrot', () => recallText('the parrot'), 'recall');
}
function buildRoomControls() {
  const items = [];
  STANDING_QUERIES.forEach((q, i) => items.push({ label: STANDING_LABELS[i], run: () => askText(q) }));
  items.push({ label: 'Something not derivable', run: () => askText(CONTROL_QUERY) });
  for (const f of (mind?.facts || []).slice(0, 3)) items.push({ label: f, run: () => askText(f), color: '#9a9790' });
  items.push({ label: 'Is a bird here?', run: () => askText('observe(Nell, Bird).') });
  items.push({ label: 'Simulate: a bird appears', run: () => { if (!eyes.watching) startEyes('simulator'); senses.simulate('Bird', true); }, color: '#9fd3c7' });
  items.push({ label: 'Simulate: the bird leaves', run: () => senses.simulate('Bird', false), color: '#9fd3c7' });
  for (const t of recallTopics.slice(0, 2)) items.push({ label: `Recall: ${t}`, run: () => recallText(t), color: '#f2b950' });
  items.push({ label: 'Recall by meaning: the parrot', run: () => recallText('the parrot'), color: '#f2b950' });
  items.push({ label: 'Load my words (640 MB to 1 GB)', run: loadWords, color: '#cfc6ff' });
  items.push({ label: 'Ask in words: who are Enma and Pity?', run: () => sayText('Who are Enma and Pity?'), color: '#cfc6ff' });
  items.push({ label: 'Build the neural index (23 MB)', run: buildNeuralIndex, color: '#cfc6ff' });
  buildControls(items);
  log(`controls: ${items.length} in the room, each one fixed query, recall or action`);
}
function buildChips() {
  const nav = el('chips'); nav.textContent = '';
  group(nav, 'Controls: each issues one fixed Nibli query. My standing, proved by the engine');
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
  if (board.userData.control) { log(`control: ${board.userData.control.label}`); board.userData.control.run(); return true; }
  if (board === standingBoard.mesh) { askText(STANDING_QUERIES[standingIndex % STANDING_QUERIES.length]); standingIndex += 1; return true; }
  if (board === answerBoard.mesh) { askText(CONTROL_QUERY); return true; }
  if (board === memoryBoard.mesh && recallTopics.length) { recallText(recallTopics[recallIndex % recallTopics.length]); recallIndex += 1; return true; }
  return false;
}

// ── vectors: recall by meaning (roadmap/N2b.md) ─────────────────────────────
const index = { space: null, store: null, items: [], embed: null, floor: 0, dim: 0, label: '' };
function englishOf(kr) { try { return back_translate_ir(kr); } catch { return kr; } }
function indexItemsNow() {
  const facts = (mind?.factItems || []).map((f) => ({ ...f, english: englishOf(f.kr) }));
  return indexItems(memory?.records || [], facts);
}
async function buildHashedIndex() {
  const t0 = performance.now();
  const items = indexItemsNow();
  const space = new HashSpace().fit(items.map((it) => it.text));
  const store = await new VectorStore({ space: space.id }).open();
  const r = await buildIndex({ items, store, embed: async (texts) => texts.map((t) => space.embed(t)) });
  const pruned = await store.pruneOthers(`${HASH_SPACE}@`).catch(() => 0);
  Object.assign(index, { space: space.id, store, items, embed: async (t) => space.embed(t), floor: HASH_FLOOR, label: `hashed space ${space.id}, no model` });
  log(`index: ${index.label}; ${items.length} items (${r.embedded} embedded, ${r.reused} reused${pruned ? `, ${pruned} rows of older corpora pruned` : ''}), store ${store.backend}${store.error ? ` (${store.error.message || store.error})` : ''}, ${Math.round(performance.now() - t0)} ms`);
  el('words').textContent = `Recall by meaning: ${items.length} records and facts in the ${index.label}, stored in ${store.backend}. The neural index is one control away.`;
}
const EMBED = {
  lib: { url: 'https://cdn.jsdelivr.net/npm/@huggingface/transformers@3.8.1/dist/transformers.min.js', sha256: 'aa5002b70e789798da263f5f99c62bd3e8fcd0c119258a493c40c180648365fa' },
  model: { id: 'Xenova/all-MiniLM-L6-v2', revision: '751bff37182d3f1213fa05d7196b954e230abad9', dtype: 'q8' },
};
let embedWorker = null; let embedSeq = 0; const embedPending = new Map();
function embedViaWorker(texts) {
  return new Promise((resolve, reject) => {
    const id = ++embedSeq;
    embedPending.set(id, { resolve, reject });
    embedWorker.postMessage({ cmd: 'embed', id, texts });
  });
}
async function buildNeuralIndex() {
  if (index.space && !index.space.startsWith(HASH_SPACE)) { status(`I'm Nell. My neural index is already built: ${index.items.length} records and facts.`, true, 't'); return; }
  if (embedWorker) return;
  const space = `${EMBED.model.id}@${EMBED.model.revision.slice(0, 7)}`;
  const t0 = performance.now();
  el('words').textContent = 'Recall by meaning: fetching the embedding library and model (pinned, hashed)…';
  embedWorker = new Worker(new URL('./embed-worker.js', import.meta.url), { type: 'module' });
  const ready = await new Promise((resolve) => {
    embedWorker.onerror = (e) => { log(`index: worker error ${e.message || e}`); resolve(false); };
    embedWorker.onmessage = (e) => {
      const m = e.data;
      if (m.status === 'progress') el('words').textContent = `Recall by meaning: ${m.label}`;
      else if (m.status === 'ready') { index.dim = m.dim; resolve(true); }
      else if (m.status === 'error') { log(`index: ${m.error}`); resolve(false); }
    };
    embedWorker.postMessage({ cmd: 'load', lib: EMBED.lib, model: EMBED.model });
  });
  if (!ready) {
    embedWorker.terminate(); embedWorker = null;
    el('words').textContent = 'Recall by meaning: the neural model could not be loaded; the hashed space stays. The log says why.';
    status("I'm Nell. I could not fetch the embedding model; my hashed index stays.", true, 'e');
    return;
  }
  embedWorker.onmessage = (e) => {
    const m = e.data; const p = embedPending.get(m.id);
    if (!p) return;
    embedPending.delete(m.id);
    if (m.status !== 'done') { p.reject(new Error(m.error)); return; }
    const out = [];
    for (let i = 0; i < m.data.length; i += m.dim) out.push(m.data.slice(i, i + m.dim));
    p.resolve(out);
  };
  const store = await new VectorStore({ space }).open();
  const items = indexItemsNow();
  const r = await buildIndex({ items, store, embed: embedViaWorker, batch: 16, onProgress: (d, n) => { el('words').textContent = `Recall by meaning: embedding ${d} of ${n}…`; } });
  Object.assign(index, { space, store, items, embed: async (t) => (await embedViaWorker([t]))[0], floor: NEURAL_FLOOR, label: `${EMBED.model.id} at ${EMBED.model.revision.slice(0, 7)}, transformers.js 3.8.1, ${index.dim} dimensions` });
  const ms = Math.round(performance.now() - t0);
  log(`index: ${index.label}; ${items.length} items (${r.embedded} embedded, ${r.reused} reused), store ${store.backend}, ${ms} ms`);
  el('words').textContent = `Recall by meaning: ${items.length} records and facts in ${index.label}; stored in ${store.backend}; ${r.reused} reused, ${r.embedded} embedded, ${ms} ms.`;
  status(`I'm Nell. My neural index is built: ${items.length} records and facts, by meaning, kept in this browser.`, true, 't');
  if (wordChips.index) wordChips.index.textContent = `Neural index: built (${items.length})`;
  journal.remember({ speaker: 'Nell', text: `Neural index built: ${items.length} items, ${index.label}; ${r.embedded} embedded, ${r.reused} reused.`, kind: 'note', about: ['room', 'index'] });
  renderJournal();
}
/** Nearest records and facts by meaning: one hit per record (its best piece), none below the floor, none in excludeIds. */
async function vectorRecall(q, k = 5, excludeIds = new Set()) {
  if (!index.store) return [];
  const qv = await index.embed(q);
  const hits = searchItems({ store: index.store, items: index.items, queryVec: qv, k: (k + excludeIds.size) * 4, floor: index.floor });
  const out = []; const seen = new Set();
  for (const h of hits) {
    const id = h.item.record ? h.item.record.id : h.key;
    if (excludeIds.has(id) || seen.has(id)) continue;
    seen.add(id); out.push(h);
    if (out.length >= k) break;
  }
  return out;
}
function hitLine(h, width) {
  const it = h.item;
  const body = it.record ? recordLine(it.record, width, it.part !== undefined && it.part > 0 ? `… ${it.chunk}` : it.chunk) : `fact · ${it.fact.english} (${it.fact.kr})`;
  return `${h.score.toFixed(2)} · ${body}`;
}

el('recall').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const q = el('recallQuery').value.trim();
  if (!q) return;
  if (!memory) { status("I'm Nell. My public memory has not arrived yet; the log says why.", true, 'e'); return; }
  const exact = recall(memory.records, q, 8);
  const near = await vectorRecall(q, 5, new Set(exact.map((r) => r.id)));
  journal.remember({ speaker: 'Visitor', text: `Recall: ${q}`, about: ['room', 'recall'] });
  if (!exact.length && !near.length) {
    status(`I'm Nell. Nothing in my public memory holds "${q}", by its words or by meaning.`, true, 'f');
    el('answer').innerHTML = `<span class="f">${escapeHtml(`Recall: ${q}`)}\n→ nothing in my memory holds that (exact text 0; by vectors above the floor 0)</span>`;
    answerBoard.update([{ text: `Recall: ${q}` }, { text: '→ nothing in my memory holds that', color: CSS.f }]);
    log(`recall: ${q} → exact 0, vectors 0`);
    journal.remember({ speaker: 'Nell', text: `Nothing in my public memory holds "${q}".`, about: ['room', 'recall'] });
    renderJournal();
    return;
  }
  const said = exact.length
    ? `${exact.length} of my records hold "${q}" in their words${near.length ? `, and ${near.length} more ${near.length === 1 ? 'is' : 'are'} near it by meaning` : ''}; the newest exact one is from ${String(exact[0].payload.timestamp).slice(0, 10)}, in the ${exact[0].peer} peer.`
    : `No record holds "${q}" in its words, but ${near.length} ${near.length === 1 ? 'is' : 'are'} near it by meaning; the nearest scores ${near[0].score.toFixed(2)}: ${hitLine(near[0], 140).slice(7)}`;
  status(`I'm Nell. ${said}`, true, 't');
  el('answer').innerHTML = `<span class="t">${escapeHtml(`Recall: ${q}`)}\n→ ${exact.length} exact, ${near.length} by vectors</span>\n` +
    (exact.length ? `Exact text:\n${exact.map((r) => escapeHtml(recordLine(r, 400))).join('\n\n')}\n\n` : '') +
    (near.length ? `<span class="fed">Nearest by vectors (${escapeHtml(index.label)}): a ranking of records that exist, never an invention; the score, then the record.\n${near.map((h) => escapeHtml(hitLine(h, 300))).join('\n\n')}</span>` : '');
  answerBoard.update([
    { text: `Recall: ${q}` }, { text: `→ ${exact.length} exact, ${near.length} by vectors`, color: CSS.t },
    ...exact.slice(0, 3).map((r) => ({ text: recordLine(r, 200) })),
    ...near.slice(0, 2).map((h) => ({ text: `≈ ${hitLine(h, 180)}`, color: '#9a9790' })),
  ]);
  log(`recall: ${q} → exact ${exact.length}, vectors ${near.length}`);
  journal.remember({ speaker: 'Nell', text: said, about: ['room', 'recall'] });
  renderJournal();
});

// ── words: the text box, fed by what the room retrieves (roadmap/N2b.md) ────
const brain = new Brain({ log });
const history = [];
async function loadWords() {
  if (brain.ready || brain.loading) return;
  el('words').textContent = 'Words: loading…';
  const ok = await brain.load((frac, label) => { el('words').textContent = `Words: ${label}${frac > 0 && frac < 1 ? ` (${Math.round(frac * 100)}%)` : ''}`; });
  if (!ok) {
    el('words').textContent = 'Words: my model could not be loaded here; the log says why. The box still answers with retrieval alone.';
    status("I'm Nell. My words could not load here; I still answer from my memory itself.", true, 'e');
    return;
  }
  el('words').textContent = `Words: ${brain.describe()} at revision ${LUCY_REV.slice(0, 7)}, loaded in this browser. Words are words; verdicts stay the engine's.`;
  status(`I'm Nell. My words are loaded: ${brain.describe()}. Ask me in words; what I say is fed from my memory and labelled as words.`, true, 't');
  if (wordChips.load) wordChips.load.textContent = `My words: loaded (${brain.engine})`;
  log(`words: loaded ${brain.describe()}, revision ${LUCY_REV}`);
}
const wordsOf = (s) => String(s).toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
/** The engine's verdicts that bear on a question: the standing lines, facts naming a constant the question names, what the eyes hold. */
function verdictLinesFor(question) {
  const lines = [];
  const say = (q) => { try { const a = ask(mind.session, q); lines.push(`${a.query} → ${verdictWord(a)}: ${a.english}`); } catch { /* not a query */ } };
  for (const q of STANDING_QUERIES) say(q);
  const words = new Set(wordsOf(question));
  for (const f of mind.facts) {
    if (lines.length >= 12) break;
    const consts = f.match(/[A-Z][A-Za-z0-9]*/g) || [];
    if (consts.some((c) => words.has(c.toLowerCase()))) say(f);
  }
  for (const cls of senses.present()) say(`observe(Nell, ${cls}).`);
  return lines;
}
const asFed = (r, text) => ({ when: String(r.payload.timestamp || '').slice(0, 10), speaker: r.payload.speaker, peer: r.peer, kind: r.payload.kind, text: text ?? r.payload.text });
el('say').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  if (!mind) return;
  const q = el('sayQuery').value.trim();
  if (!q) return;
  thinking += 1;
  try {
    journal.remember({ speaker: 'Visitor', text: q, about: ['room', 'words'] });
    const exact = memory ? recall(memory.records, q, 3) : [];
    const near = await vectorRecall(q, 5, new Set(exact.map((r) => r.id)));
    const records = [...exact.map((r) => asFed(r)), ...near.filter((h) => h.item.record).map((h) => asFed(h.item.record, h.item.chunk))];
    const facts = near.filter((h) => h.item.fact).map((h) => h.item.fact);
    const verdicts = verdictLinesFor(q);
    const context = buildContext({ verdicts, records, facts });
    const fed = `What fed this, retrieved on this device: ${exact.length} exact, ${near.length} by vectors (${index.label || 'no index yet'}), ${verdicts.length} verdicts from the engine.\n${context}`;
    const any = records.length || facts.length;
    if (!brain.ready) {
      const said = any
        ? `My words are not loaded, so here is my memory itself: ${records.length} record${records.length === 1 ? '' : 's'} and ${facts.length} fact${facts.length === 1 ? '' : 's'} bear on that. The nearest: ${records[0] ? records[0].text.replace(/\s+/g, ' ').slice(0, 160) : facts[0].english}`
        : 'My words are not loaded, and nothing in my memory bears on that.';
      status(`I'm Nell. ${said}`, true, any ? 't' : 'f');
      el('answer').innerHTML = `<span class="${any ? 't' : 'f'}">${escapeHtml(`In words: ${q}`)}\n→ retrieval alone; load my words for prose</span>\n<span class="fed">${escapeHtml(fed)}</span>`;
      answerBoard.update([{ text: `In words: ${q}`, color: '#9a9790' }, { text: said }, ...context.split('\n').slice(0, 5).map((t) => ({ text: t, color: '#9a9790' }))]);
      log(`words: ${q} → retrieval alone (${records.length} records, ${facts.length} facts, ${verdicts.length} verdicts)`);
      journal.remember({ speaker: 'Nell', text: said, about: ['room', 'words'] });
      renderJournal();
      return;
    }
    status(`I'm Nell. Let me put that into words…`, false, 'w');
    let partial = '';
    const out = await brain.say({ question: q, context, history, onToken: (piece) => {
      partial += piece;
      el('status').textContent = `In words: ${partial}`;
      bubbleBoard.update([{ text: `In words: ${partial}`, color: '#cfc6ff' }]);
    } });
    history.push({ role: 'user', content: q }, { role: 'assistant', content: out.text });
    status(`In words: ${out.text}`, true, 'w');
    el('answer').innerHTML = `<span class="t">${escapeHtml(`In words: ${q}`)}\n→ words from my model (${escapeHtml(out.engine)}, revision ${LUCY_REV.slice(0, 7)}); not a verdict</span>\n${escapeHtml(out.text)}\n\n<span class="fed">${escapeHtml(fed)}</span>`;
    answerBoard.update([
      { text: `In words: ${q}`, color: '#9a9790' }, { text: out.text, color: '#cfc6ff' },
      { text: `Words from ${out.engine}; not a verdict. Fed by ${records.length} records, ${facts.length} facts, ${verdicts.length} verdicts.`, color: '#9a9790' },
    ]);
    log(`words: ${q} → ${out.text.length} chars from ${out.engine}; prompt ${out.promptChars} chars; fed ${records.length} records, ${facts.length} facts, ${verdicts.length} verdicts`);
    journal.remember({ speaker: 'Nell', text: `In words (from my model, not a verdict): ${out.text}`, about: ['room', 'words'] });
    renderJournal();
  } catch (e) {
    status(`I'm Nell. My words failed me this time: ${e.message || e}`, true, 'e');
    log(`words: error ${e.message || e}`);
  } finally { thinking -= 1; }
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
      status(`I'm Nell. I couldn't read that as Nibli: ${e.message || e}`, true, 'e');
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
    status(`I'm Nell. ${spoken}`, true, cls);
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
