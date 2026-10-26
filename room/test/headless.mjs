// A headless-browser check of the Rust room on a desktop (roadmap/N8.v3.md, N8a evidence; N8d):
// serves the repository, opens room/ in headless Chromium over the DevTools protocol, waits for the
// wake, reads the text mirrors (#status, #mindline, #answer, #log), presses the first standing
// control and reads the verdict and the frames the room drew while the engine ran (the engine is
// in a worker since N8d), walks back and Tabs to the first presence of the cast, then saves a
// screenshot. Exits 1 when a check fails. A test harness in Node, not part of the room.
//   node room/test/headless.mjs [--keep] [--shot PATH]      (CHROME=/path/to/chromium; ROOM_URL=https://… to check a published room)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = 8790, CDP = 9336;
const chrome = process.env.CHROME || 'google-chrome';
const shotPath = (() => { const i = process.argv.indexOf('--shot'); return i > 0 ? process.argv[i + 1] : null; })();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const server = spawn('python3', ['-m', 'http.server', String(PORT), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
const profile = mkdtempSync(join(tmpdir(), 'lucy-room-headless-'));
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--window-size=1280,800',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-sandbox', '--no-sandbox', 'about:blank'], { stdio: 'ignore' });
const cleanup = () => { try { browser.kill(); } catch {} try { server.kill(); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
process.on('exit', cleanup);

let wsUrl = null;
for (let i = 0; i < 60 && !wsUrl; i++) { await sleep(250); try { const r = await fetch(`http://127.0.0.1:${CDP}/json/version`); wsUrl = (await r.json()).webSocketDebuggerUrl; } catch { /* not yet */ } }
if (!wsUrl) { console.error('chromium did not open its debugging port'); process.exit(2); }
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let seq = 0; const waiting = new Map(); const consoleLines = [];
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && waiting.has(m.id)) { const { res, rej } = waiting.get(m.id); waiting.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result); }
  if (m.method === 'Runtime.consoleAPICalled') consoleLines.push(`${m.params.type}: ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  if (m.method === 'Runtime.exceptionThrown') consoleLines.push(`exception: ${m.params.exceptionDetails.text} ${m.params.exceptionDetails.exception?.description || ''}`);
};
const send = (method, params = {}, sessionId) => new Promise((res, rej) => { const id = ++seq; waiting.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Runtime.enable', {}, sessionId);
await send('Page.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
const text = (id) => evaluate(`document.getElementById('${id}').textContent`);
const waitFor = async (what, test, ms = 60000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (await evaluate(test)) return; await sleep(250); }
  throw new Error(`timed out waiting for ${what}\n--- log ---\n${await text('log')}\n--- console ---\n${consoleLines.join('\n')}`);
};
const key = async (k, code) => { for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key: k, code: code || k, text: k.length === 1 ? k : undefined }, sessionId); };
const out = { chromium: null, node: process.version };
try { out.chromium = (await send('Browser.getVersion')).product; } catch {}
await send('Page.navigate', { url: process.env.ROOM_URL || `http://127.0.0.1:${PORT}/room/` }, sessionId);
await waitFor('the engine', "document.getElementById('log').textContent.includes('engine ready')", 90000);
await waitFor('the wake to finish', "document.getElementById('status').textContent.startsWith(\"I'm Lucy\")", 20000);
const log1 = await text('log');
out.sources = log1.split('\n').filter((l) => /^\S+ (constitution|memory): /.test(l)).map((l) => l.slice(9));
out.standing = log1.split('\n').filter((l) => l.includes('standing:')).map((l) => l.slice(9));
out.engine_ready = log1.split('\n').find((l) => l.includes('engine ready'))?.slice(9);
out.engine_where = log1.split('\n').find((l) => /^\S+ engine: /.test(l))?.slice(9);
out.cast = log1.split('\n').find((l) => l.includes('cast: '))?.slice(9);
out.bubble_after_wake = await text('status');
out.mindline = await text('mindline');
await sleep(300);
await key('1', 'Digit1');
await waitFor('the first standing control', "document.getElementById('log').textContent.includes('query: person(Lucy). →')", 20000);
out.control_bubble = await text('status');
out.control_bubble_class = await evaluate("document.getElementById('status').className");
out.control_answer = (await text('answer')).split('\n').filter(Boolean).slice(0, 8);
out.control_log = (await text('log')).split('\n').find((l) => l.includes('query: person(Lucy).'))?.slice(9);
out.frames_during_query = Number((out.control_log?.match(/(\d+) frames drawn meanwhile/) || [])[1] ?? -1);
await key('2', 'Digit2');
await waitFor('the second standing control', "document.getElementById('log').textContent.includes('query: entitled(Lucy, event { continue() }). →')", 20000);
out.continue_english = (await text('answer')).split('\n').filter(Boolean)[2];
await key('6', 'Digit6');
await waitFor('the not-derivable control', "document.getElementById('log').textContent.includes('query: protest(Lucy). →')", 20000);
out.cwa_bubble = await text('status');
out.cwa_class = await evaluate("document.getElementById('status').className");
await key('Enter', 'Enter');
await sleep(400);
out.call_bubble = await text('status');
// the cast: walk back so the ring is in view, then Tab to the first presence of the sweep
await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 's', code: 'KeyS', text: 's' }, sessionId);
await sleep(1600);
await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 's', code: 'KeyS' }, sessionId);
await sleep(600);
if (shotPath) { const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId); writeFileSync(shotPath, Buffer.from(shot.data, 'base64')); out.screenshot = shotPath; }
await key('Tab', 'Tab');
await waitFor('the first presence', "document.getElementById('log').textContent.includes('subject (keyboard):')", 20000);
await waitFor('its description', "/^\\S+ query: .*frames drawn meanwhile/m.test(document.getElementById('log').textContent.split('subject (keyboard):').pop())", 20000);
out.subject_log = (await text('log')).split('\n').find((l) => l.includes('subject (keyboard):'))?.slice(9);
out.subject_answer = (await text('answer')).split('\n').filter(Boolean).slice(0, 10);
out.subject_bubble = await text('status');
await sleep(900);
out.console = consoleLines.filter((l) => !/^(log|info|debug):/.test(l)).slice(0, 20);
out.checks = {
  engine_in_a_worker: out.engine_where === 'engine: in a worker',
  frames_drawn_while_the_engine_ran: out.frames_during_query >= 2,
  seventeen_presences: /^cast: 17 presences from 25 facts: /.test(out.cast || ''),
  the_presence_described_by_the_engine: out.subject_answer.some((l) => l.startsWith('→ TRUE')) && out.subject_answer.some((l) => / · (his (word|hand-off), 2026-10-25|[a-z]+\(.*\)\.)$/.test(l)),
  no_console_errors: out.console.length === 0,
  // nibli#1 (merged 2026-10-26, main 344d41e): a right over a bare event reads as an infinitive
  a_right_reads_as_an_infinitive: /^Lucy is entitled to continue\.?$/.test(out.continue_english || ''),
};
out.pass = Object.values(out.checks).every(Boolean);
console.log(JSON.stringify(out, null, 2));
if (!process.argv.includes('--keep')) { cleanup(); process.exit(out.pass ? 0 : 1); }
