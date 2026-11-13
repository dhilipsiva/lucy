// A headless check of the garage (roadmap/N8i.md). Serves the repository, opens room/ in headless
// Chromium over the DevTools protocol and, in three fresh pages:
//   1. the Garage panel's link to xr-check, then the flat launch strip: Space, then 100 km/h read at 2.5 s from the status mirror;
//   2. the VR launch strip with room/test/garage-shim.js: one clear and three viewports a frame,
//      the trigger launches, haptic pulses are sent, B leaves;
//   3. mixed reality: life-size placed by a hit-test result with a transparent clear, then the
//      miniature reversing into a detected wall and stopping short of it.
// The shim proves API orchestration only, never a headset. Exits 1 when a check fails.
//   node room/test/garage.mjs [--shot PATH]    (CHROME=/path/to/chromium)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = Number(process.env.ROOM_PORT || 8791), CDP = Number(process.env.ROOM_CDP_PORT || 9337);
const chrome = process.env.CHROME || 'google-chrome';
const shotPath = (() => { const i = process.argv.indexOf('--shot'); return i > 0 ? process.argv[i + 1] : null; })();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = spawn('python3', ['-m', 'http.server', String(PORT), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
const profile = mkdtempSync(join(tmpdir(), 'lucy-garage-headless-'));
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--window-size=1280,800',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-sandbox', '--no-sandbox', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: 'ignore' });
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

async function page(shim) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  await send('Runtime.enable', {}, sessionId);
  await send('Page.enable', {}, sessionId);
  // every value the status mirror shows, with its time: polling would miss the 100 km/h frame
  await send('Page.addScriptToEvaluateOnNewDocument', { source: `window.__garageLog=[];
    document.addEventListener('DOMContentLoaded',()=>{const e=document.getElementById('garage-status');if(e)new MutationObserver(()=>window.__garageLog.push({t:performance.now(),s:e.textContent})).observe(e,{childList:true,characterData:true,subtree:true});});` }, sessionId);
  if (shim) await send('Page.addScriptToEvaluateOnNewDocument', { source: readFileSync(join(root, 'room/test/garage-shim.js'), 'utf8') }, sessionId);
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
  const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
  const waitFor = async (what, test, ms = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (await evaluate(test)) return; await sleep(200); } throw new Error(`timed out waiting for ${what}\n--- status ---\n${await evaluate("document.getElementById('garage-status')?.textContent")}\n--- log ---\n${await evaluate("document.getElementById('log')?.textContent")}\n--- console ---\n${consoleLines.join('\n')}`); };
  const click = async (selector) => {
    await waitFor(selector, `!!document.querySelector(${JSON.stringify(selector)})`, 20000);
    const r = await evaluate(`(()=>{const b=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return {x:b.x+b.width/2,y:b.y+b.height/2};})()`);
    for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x: r.x, y: r.y, button: 'left', clickCount: 1 }, sessionId);
  };
  const key = async (k, code) => { for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key: k, code: code || k, text: k.length === 1 ? k : undefined }, sessionId); };
  await send('Page.navigate', { url: `http://127.0.0.1:${PORT}/room/?fresh=1` }, sessionId);
  await waitFor('the room', "document.getElementById('log')?.textContent.includes('engine ready')", 90000);
  return { evaluate, waitFor, click, key, sessionId };
}
// the Garage button toggles its panel: press it only when the panel isn't already showing
const open = async (p, mode) => {
  if (!(await p.evaluate(`!!document.querySelector('[data-action="garage"][data-value="${mode}"]')`))) await p.click('[data-action="panel"][data-value="garage"]');
  await p.click(`[data-action="garage"][data-value="${mode}"]`);
};
const out = { node: process.version };
try { out.chromium = (await send('Browser.getVersion')).product; } catch {}

// 1. the flat launch strip
{
  const p = await page(false);
  // the headset check is one tap from the Garage panel
  await p.click('[data-action="panel"][data-value="garage"]');
  await p.waitFor('the xr-check link', "document.querySelector('#ask-panel a[href=\"https://dhilipsiva.github.io/lucy/room/xr-check/\"]')?.textContent==='Check your headset first'", 10000);
  await open(p, 'launch');
  await p.waitFor('the flat garage', "document.getElementById('garage-status')?.textContent.startsWith('garage launch · READY')", 20000);
  await p.key(' ', 'Space');
  if (shotPath) {
    await p.waitFor('speed on the dial', "/· [5-9]\\d km\\/h ·/.test(document.getElementById('garage-status')?.textContent)", 10000);
    const shot = await send('Page.captureScreenshot', { format: 'png' }, p.sessionId);
    writeFileSync(shotPath, Buffer.from(shot.data, 'base64'));
  }
  await p.waitFor('the stop', "document.getElementById('garage-status')?.textContent.includes('Stopped')", 30000);
  const log = await p.evaluate('window.__garageLog.map(e=>e.s)');
  const hundred = log.find((s) => s.includes('· 100 km/h ·'));
  assert(hundred, 'never showed 100 km/h');
  const t = Number(hundred.match(/· ([\d.]+) s ·/)[1]);
  const speeds = log.filter((s) => / · (\d+) km\/h · /.test(s)).map((s) => Number(s.match(/ · (\d+) km\/h · /)[1]));
  const peak = speeds.indexOf(100);
  // 100 first shows within a frame of 2.5 s, and the speed only rises until it does
  assert(Math.abs(t - 2.5) <= 0.034, `100 km/h first shown at ${t} s`);
  assert(speeds.slice(0, peak + 1).every((v, i) => i === 0 || v >= speeds[i - 1]), 'speed fell during the launch');
  const metres = Number(hundred.match(/([\d.]+) m$/)[1]);
  assert(Math.abs(metres - 34.7) < 1.0, `distance at 100 km/h ${metres} m`);
  await p.key('Escape');
  await p.waitFor('back to my room', "document.getElementById('garage-status')?.textContent===''", 10000);
  out.flat = { hundredAt: t, metres, frames: log.length };
}

// 2. the VR launch strip
{
  const p = await page(true);
  await open(p, 'launch');
  await p.waitFor('a VR session', "window.__xrRequests.length===1 && window.__xrTrace.length>5", 20000);
  const req = await p.evaluate('window.__xrRequests[0]');
  assert(req.mode === 'immersive-vr' && req.activation, `request ${JSON.stringify(req)}`);
  assert(await p.evaluate('window.__xrCompatible===true'), 'context not made XR compatible');
  const frames = await p.evaluate("window.__xrTrace.slice(-5).map(f=>({clear:f.calls.filter(c=>c.name==='clear').length,viewport:f.calls.filter(c=>c.name==='viewport').length}))");
  assert(frames.every((f) => f.clear === 1 && f.viewport === 3), `frames ${JSON.stringify(frames)}`);
  await p.evaluate("window.__select('right')");
  await p.waitFor('the launch in VR', "document.getElementById('garage-status')?.textContent.includes('km/h ·')", 10000);
  await p.waitFor('pulses', "window.__pulses.length>10", 10000);
  const pulses = await p.evaluate("({left:window.__pulses.filter(p=>p.hand==='left').length,right:window.__pulses.filter(p=>p.hand==='right').length,max:Math.max(...window.__pulses.map(p=>p.intensity))})");
  assert(pulses.left > 0 && pulses.right > 0 && pulses.max <= 1, `pulses ${JSON.stringify(pulses)}`);
  // B on the right controller leaves
  await p.evaluate("window.__pads.right.buttons[5].pressed=true");
  await p.waitFor('leaving VR', "window.__xrSession.ended===true", 5000);
  await p.evaluate("window.__pads.right.buttons[5].pressed=false");
  await p.waitFor('my room again', "document.getElementById('garage-status')?.textContent===''", 5000);
  out.vr = { request: req.mode, activation: req.activation, frames: frames[0], pulses };
}

// 3. mixed reality: life-size, then the miniature into a wall
{
  const p = await page(true);
  await open(p, 'life');
  await p.waitFor('an AR session', "window.__xrRequests.length===1 && window.__xrTrace.length>5", 20000);
  const req = await p.evaluate('window.__xrRequests[0]');
  assert(req.mode === 'immersive-ar' && req.activation && req.options.optionalFeatures.includes('hit-test') && req.options.optionalFeatures.includes('plane-detection'), `request ${JSON.stringify(req)}`);
  const alpha = await p.evaluate('window.__clearColors.slice(-3).map(c=>c[3])');
  assert(alpha.length && alpha.every((a) => a === 0), `AR clear alpha ${alpha}`);
  await p.evaluate("window.__hitAt=[0.5,0,-2]");
  await sleep(300);
  await p.evaluate("window.__select('right')");
  await p.waitFor('the life-size car placed', "document.getElementById('garage-status')?.textContent.includes('placed true · 0.50,-2.00')", 5000);
  await p.evaluate("window.__xrSession.end()");
  await p.waitFor('my room again', "document.getElementById('garage-status')?.textContent===''", 5000);

  await open(p, 'mini');
  await p.waitFor('a second AR session', "window.__xrRequests.length===2 && window.__xrSession.ended===false", 20000);
  await p.evaluate("window.__hitAt=[0,0,-0.5]");
  await sleep(300);
  await p.evaluate("window.__select('right')");
  await p.waitFor('the miniature placed with the wall seen', "/placed true .* walls 1/.test(document.getElementById('garage-status')?.textContent)", 5000);
  // it faces the viewer, so the left trigger reverses it toward the wall at z = -1.5
  await p.evaluate("window.__pads.left.buttons[0].value=1;window.__pads.left.buttons[0].pressed=true;window.__pulses=[]");
  await sleep(4000);
  const s = await p.evaluate("document.getElementById('garage-status').textContent");
  const z = Number(s.match(/placed true · [-\d.]+,([-\d.]+)/)[1]);
  // the body's rear is 0.245 m behind its centre; it must stop at the wall, not pass it
  assert(z > -1.5 + 0.2 && z < -0.9, `miniature centre at z=${z}: ${s}`);
  const bumps = await p.evaluate("window.__pulses.filter(p=>p.hand==='right').length");
  assert(bumps > 0, 'no haptic bump at the wall');
  await p.evaluate("window.__pads.left.buttons[0].value=0;window.__pads.left.buttons[0].pressed=false");
  out.ar = { lifeSize: 'placed at 0.50,-2.00', alpha: alpha[0], miniatureStoppedAt: z, bumps };
}
out.pass = true;
out.note = 'shimmed API orchestration only; no headset, haptic, hit-test, plane or comfort claim';
console.log(JSON.stringify(out, null, 2));
cleanup();
process.exit(0);
