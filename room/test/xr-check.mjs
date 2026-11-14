// A headless check of xr-check's camera look (roadmap/N8h.v3.md). With Chrome's fake camera it
// presses "Check camera (2D)" and reads the report: every listed camera is opened by itself,
// its picture is described in numbers only, a small picture of each is on the page and not in
// the report, and every track is stopped afterwards.
// The fake camera proves orchestration only, never what a headset offers.
//   node room/test/xr-check.mjs    (CHROME=/path/to/chromium)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = Number(process.env.ROOM_PORT || 8799), CDP = Number(process.env.ROOM_CDP_PORT || 9349);
const chrome = process.env.CHROME || 'google-chrome';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = spawn('python3', ['-m', 'http.server', String(PORT), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
const profile = mkdtempSync(join(tmpdir(), 'lucy-xr-check-headless-'));
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--window-size=1280,800',
  '--no-sandbox', '--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', 'about:blank'], { stdio: 'ignore' });
const cleanup = () => { try { browser.kill(); } catch {} try { server.kill(); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
process.on('exit', cleanup);
let wsUrl = null;
for (let i = 0; i < 60 && !wsUrl; i++) { await sleep(250); try { wsUrl = (await (await fetch(`http://127.0.0.1:${CDP}/json/version`)).json()).webSocketDebuggerUrl; } catch {} }
if (!wsUrl) { console.error('chromium did not open its debugging port'); process.exit(2); }
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let seq = 0; const waiting = new Map(); const consoleLines = [];
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && waiting.has(m.id)) { const { res, rej } = waiting.get(m.id); waiting.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result); }
  if (m.method === 'Runtime.exceptionThrown') consoleLines.push(`exception: ${m.params.exceptionDetails.text} ${m.params.exceptionDetails.exception?.description || ''}`);
  if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') consoleLines.push(`error: ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
};
const send = (method, params = {}, sessionId) => new Promise((res, rej) => { const id = ++seq; waiting.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const assert = (ok, msg) => { if (!ok) throw new Error(msg + '\n--- console ---\n' + consoleLines.join('\n')); };
const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
for (const d of ['Runtime', 'Page']) await send(`${d}.enable`, {}, sessionId);
await send('Page.addScriptToEvaluateOnNewDocument', { source: `window.__streams=[];
  const gum=navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
  navigator.mediaDevices.getUserMedia=async(c)=>{const s=await gum(c);window.__streams.push(s);return s;};` }, sessionId);
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
const waitFor = async (what, expression, ms) => { const end = Date.now() + ms; while (Date.now() < end) { if (await evaluate(expression)) return; await sleep(200); } assert(false, `timed out waiting for ${what}: ${await evaluate("document.getElementById('status')?.textContent")}`); };

let out = { pass: false };
try {
  await send('Page.navigate', { url: `http://127.0.0.1:${PORT}/room/xr-check/index.html` }, sessionId);
  await waitFor('the page', "document.getElementById('status')?.textContent.startsWith('Ready')", 20000);
  await evaluate("document.getElementById('check-camera').click()");
  await waitFor('every camera looked at', "document.getElementById('status')?.textContent.startsWith('Every camera was opened')", 60000);
  const text = await evaluate("document.getElementById('json').textContent");
  const report = JSON.parse(text);
  const cams = report.camera_2d.cameras;
  const listed = report.camera_2d.devices.filter((d) => d.kind === 'videoinput').length;
  assert(listed >= 1 && cams.length === listed + 2, `cameras tried ${cams.length} for ${listed} listed`);
  const opened = cams.filter((c) => c.opened);
  assert(opened.length >= 1 && opened.every((c) => c.width > 0 && c.brightness !== null && c.detail !== null && c.change !== null), `opened cameras ${JSON.stringify(opened)}`);
  assert(cams.filter((c) => !c.opened).every((c) => typeof c.error === 'string' && c.error.length > 0), 'a camera that did not open says why');
  assert(!/data:|base64|jpeg|png/i.test(text), 'the report carries a picture');
  const thumbs = await evaluate("document.querySelectorAll('#thumbs figure canvas').length");
  assert(thumbs === opened.length, `small pictures ${thumbs} for ${opened.length} opened cameras`);
  const row = await evaluate("Array.from(document.querySelectorAll('#rows tr')).map(t=>t.textContent).find(t=>t.startsWith('a camera that shows the room'))");
  assert(row, 'no row for the cameras');
  const tracks = await evaluate("window.__streams.flatMap(s=>s.getTracks().map(t=>t.readyState))");
  assert(tracks.length >= 2 && tracks.every((t) => t === 'ended'), `camera tracks afterwards: ${tracks}`);
  assert(consoleLines.length === 0, 'console errors');
  out = { pass: true, cameras: cams.map((c) => ({ asked: c.asked, opened: c.opened, facing: c.facing, change: c.change && Math.round(c.change * 10) / 10 })), row, note: "Chrome's fake camera: orchestration only; no claim about any headset" };
} catch (e) { out.error = String(e.message || e); }
console.log(JSON.stringify(out, null, 2));
cleanup();
process.exit(out.pass ? 0 : 1);
