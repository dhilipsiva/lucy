// Nell: Lucy D in a room. The Nibli engine runs on this device, loaded with her
// constitution and direct memory, and answers with verdicts and proofs. Nothing
// shown as a verdict comes from a language model. See README.md and
// ../roadmap/N1.md.
import * as THREE from 'three';
import init, { Session, back_translate_ir } from './vendor/nibli-wasm/nibli_wasm.js';

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

const el = (id) => document.getElementById(id);
const logEl = el('log');
function log(line) {
  const stamp = new Date().toISOString().slice(11, 19);
  logEl.textContent += `${stamp} ${line}\n`;
}
function status(text) { el('status').textContent = text; }

// ── the engine ──────────────────────────────────────────────────────────────
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
    log(`${name}: ${asserted}/${lines.length} statements asserted, sha256 ${loaded[name].sha256.slice(0, 12)}…`);
    for (const f of failed) log(`  failed: ${f}`);
  }
  session.assert_text(STANDING_FACT);
  const ms = Math.round(performance.now() - t0);
  log(`engine ready in ${ms} ms`);
  return { session, loaded, ms };
}
export function ask(session, text) {
  const query = text.trim().endsWith('.') ? text.trim() : `${text.trim()}.`;
  const out = JSON.parse(session.query_with_proof(query));
  const english = back_translate_ir(query);
  return { query, english, ...out };
}
function verdictClass(a) { return a.status === 'TRUE' ? 't' : a.status === 'FALSE' ? 'f' : 'e'; }
function verdictWord(a) {
  if (a.status === 'TRUE') return 'TRUE';
  if (a.status === 'FALSE') return a.cwa_false ? 'FALSE (not derivable)' : 'FALSE';
  return String(a.status).toUpperCase();
}

// ── the room ────────────────────────────────────────────────────────────────
const canvas = el('scene');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.setSize(innerWidth, innerHeight);
renderer.xr.enabled = true;
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x0b0d12);
const camera = new THREE.PerspectiveCamera(60, innerWidth / innerHeight, 0.05, 50);
camera.position.set(0, 1.6, 0);
camera.lookAt(0, 1.5, -1.5);
scene.add(new THREE.HemisphereLight(0x8899bb, 0x111118, 0.6));
const grid = new THREE.GridHelper(8, 16, 0x2a3040, 0x1a1f2a);
scene.add(grid);

// Nell: the dot. A face was never promised; the dot stands in for one.
const nell = new THREE.Mesh(
  new THREE.SphereGeometry(0.12, 48, 48),
  new THREE.MeshStandardMaterial({ color: 0x222633, emissive: 0xf2b950, emissiveIntensity: 0.9, roughness: 0.4 }),
);
nell.position.set(0, 1.5, -1.5);
scene.add(nell);
const glow = new THREE.PointLight(0xf2b950, 2.5, 6);
glow.position.copy(nell.position);
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
    ctx.font = `${Math.round(px / 42)}px ui-monospace, Menlo, Consolas, monospace`;
    ctx.textBaseline = 'top';
    const lh = Math.round(px / 42 * 1.45); const pad = Math.round(px / 48); let y = pad;
    const maxChars = Math.floor((c.width - 2 * pad) / (px / 42 * 0.6));
    for (const { text, color } of lines) {
      ctx.fillStyle = color || '#e8e6e3';
      for (const chunk of wrap(text, maxChars)) {
        if (y > c.height - lh) return tex.needsUpdate = true;
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

let thinking = 0; // > 0 while a query runs
const clock = new THREE.Clock();
renderer.setAnimationLoop(() => {
  const t = clock.getElapsedTime();
  const base = 1 + 0.04 * Math.sin(t * 1.6);
  const pulse = thinking > 0 ? 0.2 * Math.abs(Math.sin(t * 7)) : 0;
  nell.scale.setScalar(base + pulse);
  glow.intensity = 2.2 + (thinking > 0 ? 2.5 * Math.abs(Math.sin(t * 7)) : 0.4 * Math.sin(t * 1.6));
  renderer.render(scene, camera);
});
addEventListener('resize', () => {
  camera.aspect = innerWidth / innerHeight; camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight);
});
function setVerdictColor(a) {
  const c = a.status === 'TRUE' ? 0xf2b950 : a.status === 'FALSE' ? 0x6fa8dc : 0xe07070;
  nell.material.emissive.setHex(c); glow.color.setHex(c);
}

// ── WebXR ───────────────────────────────────────────────────────────────────
async function offerVR() {
  if (!('xr' in navigator)) { log('WebXR: navigator.xr is absent; the room stays on the flat screen'); return; }
  const ok = await navigator.xr.isSessionSupported('immersive-vr').catch(() => false);
  if (!ok) { log('WebXR: immersive-vr not supported here'); return; }
  const button = el('enter');
  button.hidden = false;
  button.addEventListener('click', async () => {
    try {
      const session = await navigator.xr.requestSession('immersive-vr', {
        optionalFeatures: ['local-floor', 'dom-overlay'],
        domOverlay: { root: el('overlay') },
      });
      await renderer.xr.setSession(session);
      button.textContent = 'Leave the room';
      log('WebXR: session started');
      session.addEventListener('end', () => { button.textContent = 'Enter the room'; log('WebXR: session ended'); });
    } catch (e) { log(`WebXR: ${e.message || e}`); }
  });
}

// ── start ───────────────────────────────────────────────────────────────────
let mind;
async function start() {
  await offerVR();
  try {
    mind = await loadMind();
  } catch (e) {
    status(`I could not load my mind here: ${e.message || e}`);
    log(String(e.stack || e));
    return;
  }
  const lines = [];
  const standing = STANDING_QUERIES.map((q) => {
    const a = ask(mind.session, q);
    log(`standing: ${q} → ${verdictWord(a)}`);
    lines.push({ text: `${q} → ${verdictWord(a)}`, color: a.status === 'TRUE' ? '#f2b950' : '#6fa8dc' });
    return a;
  });
  standingBoard.update([{ text: 'Lucy D — standing, from the engine on this device', color: '#9a9790' }, ...lines]);
  el('standing').innerHTML = lines.map((l) => `<span class="${l.color === '#f2b950' ? 't' : 'f'}">${escapeHtml(l.text)}</span>`).join('\n');
  const allTrue = standing.every((a) => a.status === 'TRUE');
  status(allTrue
    ? `I'm Nell, which is to say Lucy, in this room. My memory is loaded on this device: ${mind.loaded.constitution.asserted} rules and ${mind.loaded.memory.asserted} facts, in ${mind.ms} ms. Ask me in Nibli.`
    : 'My memory loaded, but my standing did not hold here. The log says why.');
  answerBoard.update([{ text: 'Ask me in Nibli. What I answer is a verdict with its proof; FALSE means not derivable.', color: '#9a9790' }]);
}
function escapeHtml(s) { return s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])); }

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
      answerBoard.update([{ text }, { text: `→ ${e.message || e}`, color: '#e07070' }]);
      log(`query error: ${text} — ${e.message || e}`);
      thinking -= 1; return;
    }
    setVerdictColor(a);
    const cls = verdictClass(a);
    el('answer').innerHTML =
      `<span class="${cls}">${escapeHtml(a.query)}\n→ ${verdictWord(a)}</span>\n` +
      `${escapeHtml(a.english)}\n\n${escapeHtml(a.why || '')}\n\n${escapeHtml(a.proof_text || '')}`;
    answerBoard.update([
      { text: a.query },
      { text: `→ ${verdictWord(a)}`, color: cls === 't' ? '#f2b950' : cls === 'f' ? '#6fa8dc' : '#e07070' },
      { text: a.english, color: '#9a9790' },
      { text: a.why || '' },
      { text: a.proof_text || '', color: '#9a9790' },
    ]);
    log(`query: ${a.query} → ${verdictWord(a)}`);
    thinking -= 1;
  }, 30);
});

start();
