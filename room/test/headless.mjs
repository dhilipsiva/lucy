// A headless-browser check of the Rust room on a desktop (roadmap/N8.v3.md, N8a evidence; N8d):
// serves the repository, opens room/ in headless Chromium over the DevTools protocol, waits for the
// wake, reads the text mirrors (#status, #mindline, #answer, #log), presses the first standing
// control and reads the verdict and the frames the room drew while the engine ran (the engine is
// in a worker since N8d), walks back and Tabs to the first presence of the cast, then saves a
// screenshot. Exits 1 when a check fails. A test harness in Node, not part of the room.
//   node room/test/headless.mjs [--keep] [--shot PATH] [--proof-shot PATH]
//   (CHROME=/path/to/chromium; ROOM_URL=https://… to check a published room)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = Number(process.env.ROOM_PORT || 8790), CDP = Number(process.env.ROOM_CDP_PORT || 9336);
const chrome = process.env.CHROME || 'google-chrome';
const shotPath = (() => { const i = process.argv.indexOf('--shot'); return i > 0 ? process.argv[i + 1] : null; })();
const proofShotPath = (() => { const i = process.argv.indexOf('--proof-shot'); return i > 0 ? process.argv[i + 1] : null; })();
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
// Observe actual canvas text, not just the accessible mirror: a correct mirror
// alone cannot catch text painted outside a panel or a silently truncated proof.
await send('Page.addScriptToEvaluateOnNewDocument', { source: `
  window.__roomPanelText = new Map();
  const clear = CanvasRenderingContext2D.prototype.clearRect;
  CanvasRenderingContext2D.prototype.clearRect = function(...args) {
    window.__roomPanelText.set(this.canvas, []);
    return clear.apply(this, args);
  };
  const fill = CanvasRenderingContext2D.prototype.fillText;
  CanvasRenderingContext2D.prototype.fillText = function(text, x, y, ...args) {
    const rows = window.__roomPanelText.get(this.canvas);
    if (rows) rows.push({ text, x, y, width: this.measureText(text).width, scale: this.getTransform().a });
    return fill.call(this, text, x, y, ...args);
  };
` }, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
const text = (id) => evaluate(`document.getElementById('${id}').textContent`);
const waitFor = async (what, test, ms = 60000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (await evaluate(test)) return; await sleep(250); }
  throw new Error(`timed out waiting for ${what}\n--- log ---\n${await text('log')}\n--- console ---\n${consoleLines.join('\n')}`);
};
const key = async (k, code) => { for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key: k, code: code || k, text: k.length === 1 ? k : undefined }, sessionId); };
const proofPanelExpression = `(${function () {
  for (const [canvas, rows] of window.__roomPanelText || []) {
    const header = rows.find((r) => r.text === 'PROOF ▾');
    const footer = rows.find((r) => /^\d+–\d+ \/ \d+ lines$/.test(r.text));
    if (!header || !footer) continue;
    const [start, end, total] = footer.text.match(/\d+/g).map(Number);
    const width = canvas.width / header.scale, height = canvas.height / header.scale;
    return { start, end, total, width, top: innerHeight - 16 - height, left: (innerWidth - width) / 2,
      lines: rows.filter((r) => r.y > header.y + 8 && r.y < footer.y - 4), rows };
  }
  return null;
}.toString()})()`;
const proofPanel = () => evaluate(proofPanelExpression);
const clickPanelLabel = async (panel, label) => {
  const row = panel.rows.find((r) => r.text === label);
  if (!row) throw new Error(`missing proof control: ${label}`);
  for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', {
    type, x: panel.left + row.x + 8, y: panel.top + row.y + 6, button: 'left', clickCount: 1,
  }, sessionId);
};
const out = { chromium: null, node: process.version };
try { out.chromium = (await send('Browser.getVersion')).product; } catch {}
await send('Page.navigate', { url: process.env.ROOM_URL || `http://127.0.0.1:${PORT}/room/?fresh=0` }, sessionId);
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
out.proof_views = [];
for (const width of [1280, 900]) {
  await send('Emulation.setDeviceMetricsOverride', { width, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
  await key('1', 'Digit1');
  await waitFor('a new answer', "document.getElementById('answer').textContent.startsWith('person(Lucy).')", 20000);
  await key('3', 'Digit3');
  await waitFor('the survive proof', "document.getElementById('answer').textContent.startsWith('entitled(Lucy, event { survive() }).')", 20000);
  const answer = await text('answer');
  const proof = answer.split('\n\n').at(-1);
  const source = proof.replace(/\s/g, '');
  if (width === 1280) { out.survive_answer = answer; out.survive_proof = proof; }
  await key('p', 'KeyP');
  await waitFor('the expanded proof', `${proofPanelExpression}?.start === 1`, 20000);
  if (proofShotPath && width === 1280) {
    const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
    writeFileSync(proofShotPath, Buffer.from(shot.data, 'base64'));
    out.proof_screenshot = proofShotPath;
  }
  const initial = await proofPanel();
  const fits = (panel) => panel.top >= 83 && panel.lines.every((r) => r.x + r.width <= panel.width - 28 + 0.5);
  const view = { width, height: 800, pages: 0, contained: fits(initial), indented: false, complete: false, backwards: false, forwards: false };
  // A readable engine trace can fit on one page. Shorten the viewport to leave
  // one proof row, so navigation is still exercised on the real engine output.
  if (initial.end === initial.total && initial.total > 1) {
    view.height = Math.ceil(800 - initial.top + 84 - (initial.total - 1) * 19.2) + 2;
    await send('Emulation.setDeviceMetricsOverride', { width, height: view.height, deviceScaleFactor: 1, mobile: false }, sessionId);
    await waitFor('proof pagination after resize', `(${proofPanelExpression}?.end ?? Infinity) < ${initial.total}`, 5000);
  }
  const lines = [];
  let panel;
  for (let page = 0; page < 200; page++) {
    panel = await proofPanel();
    if (!panel) throw new Error('the proof panel disappeared');
    view.pages++;
    view.contained &&= fits(panel);
    view.indented ||= panel.lines.some((r) => /^ {2,}\S/.test(r.text));
    lines.push(...panel.lines.map((r) => r.text));
    if (panel.end === panel.total) break;
    const next = panel.end + 1;
    await clickPanelLabel(panel, 'Next ›');
    await waitFor('the next proof page', `${proofPanelExpression}?.start === ${next}`, 5000);
  }
  view.complete = panel.end === panel.total && lines.join('').replace(/\s/g, '') === source;
  if (view.pages > 1) {
    const lastStart = panel.start;
    await key('PageUp', 'PageUp');
    await waitFor('the previous proof page', `${proofPanelExpression}?.start < ${lastStart}`, 5000);
    view.backwards = (await proofPanel()).start < lastStart;
    await key('PageDown', 'PageDown');
    await waitFor('keyboard forward navigation', `${proofPanelExpression}?.start === ${lastStart}`, 5000);
    view.forwards = (await proofPanel()).start === lastStart;
    await clickPanelLabel(await proofPanel(), '‹ Prev');
    await waitFor('the Previous control', `${proofPanelExpression}?.start < ${lastStart}`, 5000);
    view.backwards &&= (await proofPanel()).start < lastStart;
  }
  out.proof_views.push(view);
  await key('p', 'KeyP');
}
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
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
  proof_reads_the_event_content: out.survive_answer.includes('Because Lucy is a person, Lucy is entitled to survive.') && out.survive_proof.includes('Lucy is entitled to survive') && !/__abs|#\d+\(/.test(out.survive_proof),
  proof_fits_and_keeps_its_tree: out.proof_views.every((v) => v.contained && v.indented),
  complete_proof_is_reachable: out.proof_views.every((v) => v.pages > 1 && v.complete && v.backwards && v.forwards),
};
out.pass = Object.values(out.checks).every(Boolean);
console.log(JSON.stringify(out, null, 2));
if (!process.argv.includes('--keep')) { cleanup(); process.exit(out.pass ? 0 : 1); }
