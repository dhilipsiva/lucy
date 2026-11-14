// A headless check of capturing a home and opening it again (roadmap/N8j.md, N8j.v2.md). Serves
// the repository and, with room/test/garage-shim.js and Chrome's fake camera:
//   1. starts a capture, walks the shimmed head, and reads the counts from the status mirror:
//      every camera frame is kept as taken, and no face finder runs on the "headset";
//   2. ends it with B: the camera's tracks are stopped and nothing was sent over the network;
//   3. downloads the capture and checks the file: marked raw, counts that add up, JPEG pictures;
//   4. looks at it on the flat screen, moves an alignment slider, and leaves;
//   5. adds the fixture portrait as one more raw picture and opens that file through the picker:
//      my room blurs it with the face finder's worker before storing it, and the file it then
//      hands back is marked processed, with one face blurred in the portrait.
// The shim and the fake camera prove orchestration only, never a headset.
//   node room/test/home.mjs [--keep FILE] [--shot PNG]    (CHROME=/path/to/chromium)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = Number(process.env.ROOM_PORT || 8798), CDP = Number(process.env.ROOM_CDP_PORT || 9348);
const chrome = process.env.CHROME || 'google-chrome';
const keep = (() => { const i = process.argv.indexOf('--keep'); return i > 0 ? process.argv[i + 1] : null; })();
const shotPath = (() => { const i = process.argv.indexOf('--shot'); return i > 0 ? process.argv[i + 1] : null; })();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const server = spawn('python3', ['-m', 'http.server', String(PORT), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
const profile = mkdtempSync(join(tmpdir(), 'lucy-home-headless-'));
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--window-size=1280,800',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-sandbox', '--no-sandbox', '--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', 'about:blank'], { stdio: 'ignore' });
const cleanup = () => { try { browser.kill(); } catch {} try { server.kill(); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
process.on('exit', cleanup);
let wsUrl = null;
for (let i = 0; i < 60 && !wsUrl; i++) { await sleep(250); try { wsUrl = (await (await fetch(`http://127.0.0.1:${CDP}/json/version`)).json()).webSocketDebuggerUrl; } catch {} }
if (!wsUrl) { console.error('chromium did not open its debugging port'); process.exit(2); }
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let seq = 0; const waiting = new Map(); const consoleLines = []; const requests = []; let chooser = null;
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && waiting.has(m.id)) { const { res, rej } = waiting.get(m.id); waiting.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result); }
  if (m.method === 'Runtime.exceptionThrown') consoleLines.push(`exception: ${m.params.exceptionDetails.text} ${m.params.exceptionDetails.exception?.description || ''}`);
  if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') consoleLines.push(`error: ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  if (m.method === 'Network.requestWillBeSent') requests.push({ url: m.params.request.url, method: m.params.request.method, body: !!m.params.request.hasPostData });
  if (m.method === 'Page.fileChooserOpened') chooser = m.params;
};
const send = (method, params = {}, sessionId) => new Promise((res, rej) => { const id = ++seq; waiting.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const assert = (ok, msg) => { if (!ok) throw new Error(msg + '\n--- console ---\n' + consoleLines.join('\n')); };

const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
for (const d of ['Runtime', 'Page', 'Network', 'DOM']) await send(`${d}.enable`, {}, sessionId);
await send('Page.setInterceptFileChooserDialog', { enabled: true }, sessionId);
// every camera stream the page opens, and every file it hands the visitor
await send('Page.addScriptToEvaluateOnNewDocument', { source: `window.__streams=[];window.__downloads=[];
  const gum=navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
  navigator.mediaDevices.getUserMedia=async(c)=>{const s=await gum(c);window.__streams.push(s);return s;};
  // behind an immersive session a browser may give a page no idle time, so canvas.toBlob never
  // answers there: on his Quest 3 the first build kept no picture. The capture must not need it.
  const toBlob=HTMLCanvasElement.prototype.toBlob;
  HTMLCanvasElement.prototype.toBlob=function(...a){if(window.__xrSession&&!window.__xrSession.ended){window.__starved=(window.__starved||0)+1;return;}return toBlob.apply(this,a);};
  const click=HTMLAnchorElement.prototype.click;
  HTMLAnchorElement.prototype.click=function(){if(this.download){window.__downloads.push(fetch(this.href).then(r=>r.text()));return;}return click.call(this);};` }, sessionId);
await send('Page.addScriptToEvaluateOnNewDocument', { source: readFileSync(join(root, 'room/test/garage-shim.js'), 'utf8') }, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
const status = () => evaluate("document.getElementById('garage-status')?.textContent");
const waitFor = async (what, test, ms = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (await evaluate(test)) return; await sleep(200); } throw new Error(`timed out waiting for ${what}\n--- status ---\n${await status()}\n--- notice ---\n${await evaluate("document.getElementById('ask-notice')?.textContent")}\n--- console ---\n${consoleLines.join('\n')}`); };
const click = async (selector) => {
  await waitFor(selector, `!!document.querySelector(${JSON.stringify(selector)})`, 20000);
  const r = await evaluate(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});e.scrollIntoView({block:'center'});const b=e.getBoundingClientRect();return {x:b.x+b.width/2,y:b.y+b.height/2};})()`);
  for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x: r.x, y: r.y, button: 'left', clickCount: 1 }, sessionId);
};
const open = async (what) => {
  if (!(await evaluate(`!!document.querySelector('[data-action="garage"][data-value="${what}"]')`))) await click('[data-action="panel"][data-value="garage"]');
  await click(`[data-action="garage"][data-value="${what}"]`);
};
const num = (s, key) => Number((s.match(new RegExp(`${key} (\\d+)`)) || [])[1]);
const out = { node: process.version };
try { out.chromium = (await send('Browser.getVersion')).product; } catch {}

await send('Page.navigate', { url: `http://127.0.0.1:${PORT}/room/?fresh=1` }, sessionId);
await waitFor('the room', "document.getElementById('log')?.textContent.includes('engine ready')", 90000);
requests.length = 0;

// 1. capture
await open('capture');
await waitFor('a mixed-reality session', "window.__xrRequests.length===1 && window.__xrTrace.length>5", 20000);
const req = await evaluate('window.__xrRequests[0]');
assert(req.mode === 'immersive-ar' && req.activation && ['plane-detection', 'mesh-detection'].every((f) => req.options.optionalFeatures.includes(f)) && !req.options.optionalFeatures.includes('camera-access'), `request ${JSON.stringify(req)}`);
assert((await evaluate('window.__streams.length')) === 1, 'the camera was not asked for in the same press');
for (let i = 1; i <= 6; i++) { await evaluate(`window.__headAt=[${i * 0.5 - 1.5},1.6,0.6]`); await sleep(900); }
await waitFor('pictures kept', "Number((document.getElementById('garage-status')?.textContent.match(/kept (\\d+)/)||[])[1])>=3", 20000);
const during = await status();
assert(num(during, 'planes') === 1 && num(during, 'meshes') === 1 && num(during, 'path') > 30 && during.endsWith('raw'), `capture status ${during}`);
assert(!requests.some((r) => r.url.includes('face')), 'the face finder was fetched during a capture');
assert(!(await evaluate('window.__starved')), 'the capture waited on canvas.toBlob during the session');
const alpha = await evaluate('window.__clearColors.slice(-3).map(c=>c[3])');
assert(alpha.every((a) => a === 0), `capture clear alpha ${alpha}`);

// 2. finish with B
await evaluate("window.__pads.right.buttons[5].pressed=true");
await waitFor('the session to end', "window.__xrSession.ended===true", 5000);
await evaluate("window.__pads.right.buttons[5].pressed=false");
await waitFor('the notice', "document.getElementById('ask-notice')?.textContent.startsWith('Captured your home')", 10000);
const tracks = await evaluate("window.__streams.flatMap(s=>s.getTracks().map(t=>t.readyState))");
assert(tracks.length > 0 && tracks.every((t) => t === 'ended'), `camera tracks after the capture: ${tracks}`);
// Nothing may carry capture data out: no request with a body or a method other than GET, and
// beyond this server only my room's usual read of my public peers, at fixed addresses with no
// query string to hide anything in.
const peers = /^https:\/\/(api\.github\.com\/repos\/dhilipsiva\/|raw\.githubusercontent\.com\/dhilipsiva\/)[A-Za-z0-9_.\/-]+$/;
const sent = requests.filter((r) => r.method !== 'GET' || r.body || !(r.url.startsWith(`http://127.0.0.1:${PORT}/`) || peers.test(r.url)));
assert(sent.length === 0, `the capture reached the network: ${JSON.stringify(sent)}`);
// he must be told where the file is: the notice names the button, and the place he exports
// the conversation says the capture is separate and offers it
const told = await evaluate("document.getElementById('ask-notice').textContent");
assert(told.includes('Garage → Download my capture') && told.includes('does not include it'), `the notice doesn't say where the file is: ${told}`);
await click('[data-action="panel"][data-value="more"]');
await waitFor('the capture offered beside Export', `(()=>{const b=document.querySelector('.ask-row [data-action="garage"][data-value="download"]');return !!b&&/Your captured home \\(1 surfaces, \\d+ pictures\\) is separate/.test(b.parentElement.textContent)&&!!document.querySelector('[data-action="export"][data-value="all"]')?.textContent.includes('Export conversation');})()`, 10000);
out.capture = { status: during, notice: await evaluate("document.getElementById('ask-notice').textContent"), tracks, requestsDuring: requests.length };

// 3. download and read the file
await sleep(500);
await open('download');
await waitFor('the download', 'window.__downloads.length===1', 20000);
const text = await evaluate('window.__downloads[0]');
const file = JSON.parse(text);
assert(file.format === 'lucy-home-capture-v1' && file.raw === true, `format ${file.format} raw ${file.raw}`);
assert(file.planes.length === 1 && file.planes[0].label === 'wall' && file.meshes.length === 1 && file.meshes[0].indices.length === 3, 'surfaces in the file');
assert(file.photos.length >= 5 && file.photos.every((p) => p.jpeg_base64.startsWith('/9j/') && p.pose.length === 16 && p.faces_blurred === 0), 'pictures in the file');
assert(file.stats.kept === file.photos.length && file.stats.kept + file.stats.dropped === file.stats.offered, `counts ${JSON.stringify(file.stats)}`);
assert(file.detector.name === '' && file.stats.faces_blurred === 0, `a raw capture names no detector: ${JSON.stringify(file.detector)}`);
const path = keep || join(profile, 'lucy-home-capture.json');
writeFileSync(path, text);
out.file = { bytes: text.length, photos: file.photos.length, stats: file.stats, pathPoints: file.path.length };

// 4. look at it on this screen, move a slider, leave
await open('home');
await waitFor('the home on screen', "/garage home · planes 1 · meshes 1 · pictures \\d+ · cells \\d+ · textured [1-9]/.test(document.getElementById('garage-status')?.textContent)", 20000);
const shown = await status();
if (shotPath) { const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId); writeFileSync(shotPath, Buffer.from(shot.data, 'base64')); }
assert(await evaluate("!!document.getElementById('home-panel')"), 'no alignment panel');
await evaluate("(()=>{const i=document.getElementById('home-hfov_deg');i.value='50';i.dispatchEvent(new Event('input',{bubbles:true}));})()");
await waitFor('a re-paint', `document.getElementById('garage-status')?.textContent!==${JSON.stringify(shown)}`, 5000);
const narrowed = await status();
assert(num(narrowed, 'textured') < num(shown, 'textured'), `a narrower lens should cover less: ${shown} → ${narrowed}`);
await click('#home-panel [data-role="leave"]');
await waitFor('my room again', "document.getElementById('garage-status')?.textContent===''", 5000);
assert(!(await evaluate("!!document.getElementById('home-panel')")), 'the panel stayed');
out.view = { shown, narrowed };

// 5. a raw file with a face in it, opened through the picker: blurred before it is stored
const withFace = JSON.parse(text);
const portraitId = Math.max(...withFace.photos.map((p) => p.id)) + 1;
withFace.photos.push({ ...withFace.photos[0], id: portraitId, jpeg_base64: readFileSync(join(root, 'room/face/tests/portrait.jpg')).toString('base64') });
withFace.stats.kept += 1; withFace.stats.offered += 1;
const facePath = join(profile, 'with-face.json');
writeFileSync(facePath, JSON.stringify(withFace));
chooser = null;
await open('open');
for (let i = 0; i < 50 && !chooser; i++) await sleep(100);
assert(chooser, 'no file chooser opened');
await send('DOM.setFileInputFiles', { files: [facePath], backendNodeId: chooser.backendNodeId }, sessionId);
await waitFor('the opened home', "/garage home · planes 1 · meshes 1 · pictures \\d+/.test(document.getElementById('garage-status')?.textContent)", 60000);
out.reopened = await status();
assert(num(out.reopened, 'pictures') === withFace.photos.length, 'the reopened home lost pictures');
assert(requests.some((r) => r.url.endsWith('/room/face-worker.js')), 'the face finder was not used to open a raw file');
await click('#home-panel [data-role="leave"]');
await waitFor('my room again', "document.getElementById('garage-status')?.textContent===''", 5000);
await open('download');
await waitFor('the second download', 'window.__downloads.length===2', 20000);
const processed = JSON.parse(await evaluate('window.__downloads[1]'));
const portrait = processed.photos.find((p) => p.id === portraitId);
assert(processed.raw === false && processed.detector.name === 'ultraface-rfb-320' && /^[0-9a-f]{64}$/.test(processed.detector.model_sha256), `processed file ${processed.raw} ${JSON.stringify(processed.detector)}`);
// the portrait's one face is blurred; the finder errs toward too many, so the fake camera's
// test pattern may add a mistaken box or two, and the total says how many
assert(portrait && portrait.faces_blurred === 1 && processed.stats.faces_blurred === processed.photos.reduce((n, p) => n + p.faces_blurred, 0), `the portrait's face: ${JSON.stringify(portrait && portrait.faces_blurred)} of ${processed.stats.faces_blurred}`);
assert(portrait.jpeg_base64 !== withFace.photos.at(-1).jpeg_base64, 'the portrait came back unchanged');
out.blurredInBrowser = { pictures: processed.photos.length, faces: processed.stats.faces_blurred, detector: processed.detector.name };

out.pass = true;
out.note = 'shimmed headset and fake camera: orchestration only; no headset, frame-rate, alignment or texture-quality claim';
console.log(JSON.stringify(out, null, 2));
cleanup();
process.exit(0);
