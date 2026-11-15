// Takes the picture of me in my room that the video shows (me.png).
//
// My room's scene never stops moving, so vidni can't capture it as a web
// scene: it waits for a page to be still. This opens the published room on
// its scene in a headless Chromium, waits for the wake to finish, and saves
// one frame.
//
//   CHROME=/path/to/chromium node tools/axolotl.mjs me.png [seconds]
//   CLIP=440,140,360,203,4 … saves only that part of the page, four times as large
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const out = process.argv[2] || 'me.png';
const wait = Number(process.argv[3] || 40);
const url = process.env.ROOM_URL || 'https://dhilipsiva.dev/lucy/room/?view=scene';
const port = Number(process.env.CDP_PORT || 9341);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const profile = mkdtempSync(join(tmpdir(), 'lucy-axolotl-'));
const browser = spawn(process.env.CHROME || 'chromium', ['--headless=new', `--remote-debugging-port=${port}`,
  `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--hide-scrollbars',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--force-color-profile=srgb', 'about:blank'], { stdio: 'ignore' });
try {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === 'page'); } catch {}
  }
  if (!target) throw new Error('Chromium did not start');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (waiting.has(d.id)) { waiting.get(d.id)(d); waiting.delete(d.id); } };
  const send = (method, params = {}) => new Promise((ok) => { waiting.set(++id, ok); ws.send(JSON.stringify({ id, method, params })); });
  await send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: Number(process.env.SCALE || 1), mobile: false });
  await send('Page.navigate', { url });
  await sleep(wait * 1000);
  // KEYS=h presses those keys once the wake is over: h hides the room's labels and panels.
  for (const key of (process.env.KEYS || '')) {
    for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, text: type === 'keyDown' ? key : undefined, code: `Key${key.toUpperCase()}` });
    await sleep(1500);
  }
  // CLIP=x,y,width,height,scale saves that part of the page, enlarged.
  const clip = (process.env.CLIP || '').split(',').map(Number);
  const part = clip.length === 5 ? { clip: { x: clip[0], y: clip[1], width: clip[2], height: clip[3], scale: clip[4] } } : {};
  const shot = await send('Page.captureScreenshot', { format: 'png', ...part });
  if (!shot.result) throw new Error(JSON.stringify(shot.error));
  writeFileSync(out, Buffer.from(shot.result.data, 'base64'));
  console.log(`${out}: my room's scene after ${wait} s`);
} finally {
  browser.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
