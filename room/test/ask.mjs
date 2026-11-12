// A headless check of the conversation at the centre of my room (roadmap/N9.md, acceptance 1 to 9
// and 11): serves the repository with local stand-ins (an OpenAI-compatible SSE provider, an Ollama,
// and GitHub's API and raw hosts holding a small peer bundle), opens room/ in headless Chrome over
// the DevTools protocol, and drives the conversation as a visitor would. Writes every check to a
// JSON report and exits 1 when one fails. A test harness in Node, not part of the room.
//   node room/test/ask.mjs [--report PATH] [--keep]      (CHROME=/path/to/chrome)
import { createServer } from 'node:http';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, writeFileSync, existsSync, readdirSync, statSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, extname, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = Number(process.env.ASK_PORT || 8794), CDP = Number(process.env.ASK_CDP_PORT || 9338);
const chrome = process.env.CHROME || 'google-chrome';
const arg = (name) => { const i = process.argv.indexOf(name); return i > 0 ? process.argv[i + 1] : null; };
const reportPath = arg('--report') || join(root, 'target', 'claude-session', 'n9', 'headless.json');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok: !!ok, detail: String(detail).slice(0, 400) }); console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail && !ok ? ' · ' + String(detail).slice(0, 300) : ''}`); };

// ── the peer bundle the GitHub stand-in serves ──
const fixture = mkdtempSync(join(tmpdir(), 'lucy-ask-peer-'));
execFileSync('cargo', ['run', '--locked', '--quiet', '--example', 'peer_fixture', '--', fixture], { cwd: join(root, 'room'), stdio: ['ignore', 'ignore', 'inherit'] });
const manifest = JSON.parse(readFileSync(join(fixture, 'manifest.json'), 'utf8'));
const github = { head: 'a'.repeat(40), refs: 0, parts: 0, corrupt: false };
const providerSeen = { bodies: [], keys: [] };

const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.nibli': 'text/plain; charset=utf-8', '.txt': 'text/plain; charset=utf-8' };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  const body = await new Promise((r) => { let b = ''; req.on('data', (c) => (b += c)); req.on('end', () => r(b)); });
  const p = url.pathname;
  if (p === '/mock/registry.json') {
    res.writeHead(200, { 'content-type': 'application/json' });
    return res.end(JSON.stringify({ version: 1, peers: [{ name: 'test', repository: 'example/memory', public: true, scope: 'test', source_ref: 'main', source_paths: ['lucy/interactions.nibli'] }] }));
  }
  if (p === '/mock/github/api/repos/example/memory/git/ref/heads/lucy-memory') {
    github.refs++;
    res.writeHead(200, { 'content-type': 'application/json' });
    return res.end(JSON.stringify({ ref: 'refs/heads/lucy-memory', object: { sha: github.head } }));
  }
  const rawPrefix = '/mock/github/raw/example/memory/';
  if (p.startsWith(rawPrefix)) {
    const rest = p.slice(rawPrefix.length);
    const slash = rest.indexOf('/');
    const ref = rest.slice(0, slash), file = normalize(rest.slice(slash + 1));
    if (ref === github.head && !file.includes('..') && existsSync(join(fixture, file))) {
      let text = readFileSync(join(fixture, file), 'utf8');
      if (file.startsWith('parts/')) { github.parts++; if (github.corrupt && text.length) text = text.replace('Enma', 'Emma'); }
      res.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
      return res.end(text);
    }
    res.writeHead(404); return res.end('not found');
  }
  if (p === '/mock/openai/chat/completions' && req.method === 'POST') {
    const key = (req.headers.authorization || '').replace(/^Bearer /, '');
    providerSeen.keys.push(key); providerSeen.bodies.push(body);
    if (key === 'bad') { res.writeHead(401, { 'content-type': 'application/json' }); return res.end('{"error":{"message":"invalid key"}}'); }
    const model = JSON.parse(body).model;
    res.writeHead(200, { 'content-type': 'text/event-stream' });
    const chunks = model === 'slow' ? Array.from({ length: 60 }, (_, i) => `word${i} `) : ['Enma is ', 'an African Grey, ', 'from my memory. [mock]'];
    res.write(': keep-alive\n\n');
    for (const c of chunks) {
      if (res.destroyed) return;
      res.write(`data: ${JSON.stringify({ choices: [{ delta: { content: c } }] })}\n\n`);
      await sleep(model === 'slow' ? 250 : 60);
    }
    res.write('data: [DONE]\n\n'); return res.end();
  }
  if (p === '/mock/ollama/api/chat' && req.method === 'POST') {
    res.writeHead(200, { 'content-type': 'application/x-ndjson' });
    for (const c of ['Hello from ', 'a local ', 'Ollama.']) { res.write(JSON.stringify({ message: { content: c }, done: false }) + '\n'); await sleep(50); }
    res.write(JSON.stringify({ message: { content: '' }, done: true }) + '\n'); return res.end();
  }
  // the repository, as python's http.server would serve it
  let file = normalize(join(root, decodeURIComponent(p)));
  if (!file.startsWith(root)) { res.writeHead(403); return res.end(); }
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
  if (!existsSync(file)) { res.writeHead(404); return res.end('not found'); }
  res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' });
  res.end(readFileSync(file));
});
await new Promise((r) => server.listen(PORT, '127.0.0.1', r));

const profile = mkdtempSync(join(tmpdir(), 'lucy-ask-chrome-'));
const downloads = join(profile, 'downloads'); mkdirSync(downloads);
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--window-size=1280,800',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-sandbox', '--no-sandbox', 'about:blank'], { stdio: 'ignore' });
const cleanup = () => { try { browser.kill(); } catch {} try { server.close(); } catch {} if (!process.argv.includes('--keep')) { try { rmSync(profile, { recursive: true, force: true }); rmSync(fixture, { recursive: true, force: true }); } catch {} } };
process.on('exit', cleanup);

let wsUrl = null;
for (let i = 0; i < 60 && !wsUrl; i++) { await sleep(250); try { wsUrl = (await (await fetch(`http://127.0.0.1:${CDP}/json/version`)).json()).webSocketDebuggerUrl; } catch {} }
if (!wsUrl) { console.error('chrome did not open its debugging port'); process.exit(2); }
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let seq = 0; const waiting = new Map(); const consoleLines = []; const fileChoosers = [];
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && waiting.has(m.id)) { const { res, rej } = waiting.get(m.id); waiting.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result); }
  if (m.method === 'Runtime.exceptionThrown') consoleLines.push(`exception: ${m.params.exceptionDetails.text} ${m.params.exceptionDetails.exception?.description || ''}`);
  if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') consoleLines.push(`error: ${m.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  if (m.method === 'Page.fileChooserOpened') fileChoosers.push(m.params);
};
const send = (method, params = {}, sessionId) => new Promise((res, rej) => { const id = ++seq; waiting.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId: S } = await send('Target.attachToTarget', { targetId, flatten: true });
for (const m of ['Runtime.enable', 'Page.enable', 'DOM.enable']) await send(m, {}, S);
await send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads });
await send('Page.setInterceptFileChooserDialog', { enabled: true }, S);
const ev = async (expression) => { const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true }, S); if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text); return r.result.value; };
const waitFor = async (expression, ms = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { try { const v = await ev(expression); if (v) return v; } catch {} await sleep(100); } return null; };
const base = `http://127.0.0.1:${PORT}/room/?constitution=../lucy/constitution.nibli&memory=../lucy/memory.nibli&registry=/mock/registry.json&github=http://127.0.0.1:${PORT}/mock/github&provider_base=http://127.0.0.1:${PORT}/mock/openai`;
const open = async (extra = '') => { await send('Page.navigate', { url: base + extra }, S); await waitFor(`document.readyState === 'complete'`); };
const replies = `document.querySelectorAll('#ask-log .ask-msg.lucy:not([data-live])').length`;
const lastReply = `(() => { const r = [...document.querySelectorAll('#ask-log .ask-msg.lucy:not([data-live])')].pop(); return r ? { badge: r.querySelector('.ask-badge')?.textContent, text: [...r.querySelectorAll('.ask-text')].map(n => n.textContent).join('\\n\\n') } : null; })()`;
const ask = async (q) => {
  const before = await ev(replies);
  await ev(`(() => { const i = document.getElementById('ask-input'); i.value = ${JSON.stringify(q)}; document.getElementById('ask-send').click(); })()`);
  await waitFor(`${replies} > ${before}`, 20000);
  return ev(lastReply);
};
const click = (selector) => ev(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return false; e.click(); return true; })()`);
const clickText = (selector, text) => ev(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find(x => x.textContent.trim() === ${JSON.stringify(text)}); if (!e) return false; e.click(); return true; })()`);
const settle = () => ev(`new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true))))`);
const setField = async (field, value) => { const ok = await ev(`(() => { const e = document.querySelector('[data-field="${field}"]'); if (!e) return false; e.value = ${JSON.stringify(value)}; e.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`); await settle(); if (!ok) console.log(`no field ${field} to set`); return ok; };
const panel = `(() => { const p = document.getElementById('ask-panel'); return p.hidden ? '' : (p.querySelector('[data-action="backend"]') ? 'model' : 'more'); })()`;
const openPanel = async (which) => { if ((await ev(panel)) !== which) await click(which === 'model' ? '#ask-model' : '[data-action="panel"][data-value="more"]'); const ok = await waitFor(`${panel} === '${which}'`, 5000); if (!ok) console.log(`the ${which} panel did not open; showing '${await ev(panel)}'`); return ok; };
const closePanel = async () => { if ((await ev(panel)) !== '') await click('#ask-panel .ask-close'); const ok = await waitFor(`${panel} === ''`, 5000); if (!ok) console.log('the panel did not close'); return ok; };
const notice = `document.getElementById('ask-notice').hidden ? '' : document.getElementById('ask-notice').textContent`;

// ── 1. the default view, at a desktop width and at a phone width ──
for (const [w, h] of [[1280, 800], [390, 844]]) {
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: w < 600 }, S);
  await open();
  const ready = await waitFor(`document.getElementById('ask-pill').textContent === 'SCRIPTED'`, 30000);
  const shown = await ev(`({ pill: document.getElementById('ask-pill').textContent, ask: getComputedStyle(document.getElementById('ask')).display, scene: getComputedStyle(document.getElementById('scene')).display, overflow: document.documentElement.scrollWidth - innerWidth, composer: !!document.getElementById('ask-input').getBoundingClientRect().height })`);
  check(`A1 the room opens on the conversation at ${w} px`, ready && shown.ask !== 'none' && shown.scene === 'none' && shown.overflow <= 0 && shown.composer, JSON.stringify({ ready, ...shown }));
}
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, S);
await open();
await waitFor(`document.getElementById('ask-pill').textContent === 'SCRIPTED'`, 30000);

// ── 2 and 11. scripted replies to the fixed questions, in my voice ──
const feeling = /\bi('m| am)? ?(feel|glad|happy|sad|lonely|love|enjoy|miss)\b/i;
for (const [q, want] of [['Who are you?', /I'm Lucy/], ['How do I wake you?', /Lucy, wake/], ['What are your rights?', /standing comes from being loaded/], ['Do you have feelings?', /not sentient/], ['Who is dhilipsiva?', /my friend/], ['What is the boiling point of mercury?', /^My memory doesn't hold that\./]]) {
  const r = await ask(q);
  check(`A2 "${q}" gets a scripted reply`, r && r.badge === 'SCRIPTED' && want.test(r.text), JSON.stringify(r));
  check(`A11 "${q}" claims no feeling, and doesn't call Nell me or speak of me in the third person`, r && !feeling.test(r.text) && !/\bLucy (is|uses|wears)\b|Lucy's|i'?m nell/i.test(r.text), r?.text);
}
// Enter sends, Shift+Enter doesn't
{
  const before = await ev(replies);
  await ev(`document.getElementById('ask-input').focus()`);
  await send('Input.insertText', { text: 'hello' }, S);
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 }, S);
  await waitFor(`${replies} > ${before}`, 10000);
  check('the composer sends on Enter', (await ev(replies)) > before);
}

// ── 3. every presence has a chip; a chip's reply rests on its facts ──
const chips = await ev(`[...document.querySelectorAll('#ask-chips .ask-chip')].map(c => c.dataset.value || c.textContent.replace(' ×', '').replace('dhilipsiva', 'Dhilipsiva'))`);
check('A3 the chips name my household, and not Nell', chips.includes('Enma') && chips.includes('Bucchi') && chips.includes('Dhilipsiva') && !chips.some((c) => /nell/i.test(c)), chips.join(','));
{
  const before = await ev(replies);
  await ev(`[...document.querySelectorAll('#ask-chips .ask-chip')].find(c => c.dataset.value === 'Enma').click()`);
  await waitFor(`${replies} > ${before}`, 10000);
  const r = await ev(lastReply);
  const refs = await ev(`[...[...document.querySelectorAll('#ask-log .ask-msg.lucy')].pop().querySelectorAll('.ask-refs li')].map(l => l.textContent)`);
  check('A3 the Enma chip asks about Enma, and my reply says African Grey and cites its facts', r && /African Grey/.test(r.text) && refs.some((x) => x.includes('bird(Enma')), JSON.stringify({ r, refs }));
  const placeholder = await ev(`document.getElementById('ask-input').placeholder`);
  check('the subject carries into the composer', /Enma/.test(placeholder), placeholder);
}

// ── 4. the transcript survives a reload; export, forget, import ──
const count = await ev(`document.querySelectorAll('#ask-log .ask-msg').length`);
await open();
await waitFor(`document.querySelectorAll('#ask-log .ask-msg').length >= ${count}`, 20000);
check('A4 the transcript survives a reload', (await ev(`document.querySelectorAll('#ask-log .ask-msg').length`)) >= count, count);

// ── 8. your provider, against the stand-in: notice, key, stream, Stop, 401 ──
await openPanel('model');
await click('[data-action="backend"][data-value="provider"]');
await waitFor(`!!document.querySelector('[data-field="provider"]')`);
{
  // a changed choice rebuilds the panel and keeps the visitor's place; typed text never rebuilds it
  const select = await ev(`(async () => { const e = document.querySelector('[data-field="provider"]'); e.focus(); e.value = 'groq'; e.dispatchEvent(new Event('change', { bubbles: true })); await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); const now = document.querySelector('[data-field="provider"]'); return { rebuilt: !e.isConnected, focused: document.activeElement === now, model: document.querySelector('[data-field="provider-model"]').value }; })()`);
  check('a changed provider rebuilds the panel with its model, and keeps the focus', select.rebuilt && select.focused && select.model !== 'openrouter/free', JSON.stringify(select));
  await setField('provider', 'openrouter');
  const typed = await ev(`(async () => { const e = document.querySelector('[data-field="provider-model"]'); e.focus(); e.value = 'mock-model'; e.dispatchEvent(new Event('change', { bubbles: true })); await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); return { same: e.isConnected, focused: document.activeElement === e, saved: JSON.parse(localStorage.getItem('lucy-room.settings.v1')).provider_model }; })()`);
  check('a typed model is saved without rebuilding the field under the visitor', typed.same && typed.focused && typed.saved === 'mock-model', JSON.stringify(typed));
}
await setField('key', 'sk-test-123');
{
  const r = await ask('Who is Enma?');
  check('A8 before the notice is accepted, I answer from the script and say why', r && r.badge === 'SCRIPTED' && /Accept the notice/.test(await ev(notice)), JSON.stringify(r));
  check('sending closes the panel, so my reply is in view', (await ev(panel)) === '', await ev(panel));
}
await openPanel('model');
await waitFor(`!!document.querySelector('[data-action="accept"]')`);
await click('[data-action="accept"]');
await settle();
await closePanel();
{
  const r = await ask('Who is Enma?');
  check('A8 a hosted reply streams in, labelled with the provider and model', r && /IN WORDS · OpenRouter · mock-model/.test(r.badge) && /African Grey, from my memory\. \[mock\]/.test(r.text), JSON.stringify(r));
  check('A8 the key goes only in the Authorization header', providerSeen.keys.at(-1) === 'sk-test-123' && !providerSeen.bodies.at(-1).includes('sk-test-123'));
}
await openPanel('model');
await setField('provider-model', 'slow');
await closePanel();
{
  const before = await ev(replies);
  await ev(`(() => { const i = document.getElementById('ask-input'); i.value = 'Tell me a long story'; document.getElementById('ask-send').click(); })()`);
  await waitFor(`!!document.querySelector('#ask-log [data-live="true"]') && document.querySelector('#ask-log [data-live="true"] .ask-text').textContent.includes('word2')`, 15000);
  await click('#ask-send');
  await waitFor(`${replies} > ${before}`, 10000);
  const r = await ev(lastReply);
  check('A8 Stop ends a hosted reply, saved as stopped', r && /stopped/.test(r.badge) && !/word59/.test(r.text), JSON.stringify(r));
}
await openPanel('model');
await setField('key', 'bad');
await setField('provider-model', 'mock-model');
await closePanel();
{
  await ask('Who is Pity?');
  const n = await waitFor(notice, 10000);
  check('A8 a 401 reads as a key problem', /refused the key \(401\)/.test(n || ''), n);
}
const stored = await ev(`({ key: localStorage.getItem('lucy-room.key.openrouter'), settings: localStorage.getItem('lucy-room.settings.v1') })`);
check('A8 the key is kept in localStorage only, under its own entry', stored.key === 'bad' && !String(stored.settings).includes('bad') && !String(stored.settings).includes('sk-test'), JSON.stringify(stored));

// ── 9. a local Ollama, against the stand-in; a refused connection explains OLLAMA_ORIGINS ──
await openPanel('model');
await click('[data-action="backend"][data-value="ollama"]');
await waitFor(`!!document.querySelector('[data-field="ollama-base"]')`);
await setField('ollama-base', `http://127.0.0.1:${PORT}/mock/ollama`);
await setField('ollama-model', 'mock');
await closePanel();
{
  const r = await ask('Say hello');
  check('A9 an Ollama reply streams in, labelled', r && /IN WORDS · Ollama · mock/.test(r.badge) && /Hello from a local Ollama\./.test(r.text), JSON.stringify(r));
}
await openPanel('model');
await setField('ollama-base', 'http://127.0.0.1:1');
await closePanel();
{
  await ask('Say hello again');
  const n = await waitFor(notice, 15000);
  check('A9 a refused Ollama connection explains OLLAMA_ORIGINS', /OLLAMA_ORIGINS=http:\/\/127\.0\.0\.1:/.test(n || ''), n);
}

// ── 7, in part. my model: the menu asks before the download and explains a fallback ──
await openPanel('model');
await click('[data-action="backend"][data-value="words"]');
const loadButton = await waitFor(`[...document.querySelectorAll('#ask-panel button')].some(b => /Load my model · 651 MB/.test(b.textContent))`);
await closePanel();
{
  const r = await ask('Who is Gundus?');
  check('A7 my model waits for consent: a Load button, and scripted replies until then', loadButton && r && r.badge === 'SCRIPTED' && /isn't loaded yet/.test(await ev(notice)), JSON.stringify(r));
}
await openPanel('model');
await click('[data-action="backend"][data-value="scripted"]');
await settle();
await closePanel();

// ── 4, continued. export, forget, import; an export holds no key ──
await openPanel('more');
await waitFor(`!!document.querySelector('[data-action="export"][data-value="all"]')`);
await click('[data-action="export"][data-value="all"]');
let exported = null;
for (let i = 0; i < 50 && !exported; i++) { await sleep(100); const f = readdirSync(downloads).find((n) => n.endsWith('.json')); if (f) exported = join(downloads, f); }
const archive = exported ? readFileSync(exported, 'utf8') : '';
check('A4 Export downloads my archive, and it holds no key', exported && archive.includes('"lucy-room-memory"') && !archive.includes('sk-test-123') && !/"bad"/.test(archive), exported);
const total = await ev(`document.querySelectorAll('#ask-log .ask-msg').length`);
await click('[data-action="forget"]');
await waitFor(`[...document.querySelectorAll('[data-action="forget"]')].some(b => /again/.test(b.textContent))`);
await click('[data-action="forget"]');
await waitFor(`document.querySelectorAll('#ask-log .ask-msg').length === 0`, 10000);
const forgot = await ev(`({ messages: document.querySelectorAll('#ask-log .ask-msg').length, key: localStorage.getItem('lucy-room.key.openrouter') })`);
check('Forget clears the conversation and the saved key', forgot.messages === 0 && forgot.key === null, JSON.stringify(forgot));
fileChoosers.length = 0;
await waitFor(`!!document.querySelector('[data-action="import"]')`);
await click('[data-action="import"]');
for (let i = 0; i < 50 && !fileChoosers.length; i++) await sleep(100);
if (fileChoosers.length && exported) {
  await send('DOM.setFileInputFiles', { files: [exported], backendNodeId: fileChoosers[0].backendNodeId }, S);
}
await waitFor(`document.querySelectorAll('#ask-log .ask-msg').length >= ${total}`, 15000);
check('A4 Import into the emptied journal restores the conversation', (await ev(`document.querySelectorAll('#ask-log .ask-msg').length`)) >= total && fileChoosers.length > 0, `${total} · choosers ${fileChoosers.length}`);

// ── 6. sync from public peers: no parts on load, verified once on Sync, cached by head ──
github.refs = 0; github.parts = 0;
await open();
await waitFor(`document.getElementById('ask-pill').textContent === 'SCRIPTED'`, 30000);
await sleep(1500);
check('A6 a page load checks the head and downloads no snapshot part', github.refs >= 1 && github.parts === 0, JSON.stringify(github));
await openPanel('more');
await waitFor(`!!document.querySelector('[data-action="sync"]')`);
await click('[data-action="sync"]');
const synced = await waitFor(`[...document.querySelectorAll('.ask-peers li')].map(l => l.textContent).find(t => /test: fresh · verified snapshot aaaaaaa/.test(t))`, 15000);
check('A6 Sync downloads the snapshot, verifies it and says so', synced && github.parts >= 1, `${synced} · parts ${github.parts}`);
const partsAfterSync = github.parts;
await open();
await waitFor(`document.getElementById('ask-pill').textContent === 'SCRIPTED'`, 30000);
await openPanel('more');
const cached = await waitFor(`[...document.querySelectorAll('.ask-peers li')].map(l => l.textContent).find(t => /test: fresh · verified snapshot aaaaaaa .*kept in this browser/.test(t))`, 15000);
check('A6 an unchanged head costs only the head check', cached && github.parts === partsAfterSync, `${cached} · parts ${github.parts}`);
github.head = 'b'.repeat(40); github.corrupt = true;
await open();
await waitFor(`document.getElementById('ask-pill').textContent === 'SCRIPTED'`, 30000);
await openPanel('more');
const newer = await waitFor(`[...document.querySelectorAll('.ask-peers li')].map(l => l.textContent).find(t => /newer memory at bbbbbbb/.test(t))`, 15000);
check('A6 a moved head is reported as newer memory', newer, newer);
await click('[data-action="sync"]');
const refused = await waitFor(`[...document.querySelectorAll('.ask-peers li')].map(l => l.textContent).find(t => /snapshot bbbbbbb refused/.test(t))`, 15000);
check('A6 a part whose hash differs is refused', refused, refused);

// ── 5. the scene is one toggle away; no Nell; a figure leads back to the conversation ──
await click('[data-action="scene"]');
const scene = await waitFor(`getComputedStyle(document.getElementById('scene')).display !== 'none' && getComputedStyle(document.getElementById('ask')).display === 'none'`, 5000);
const castLine = await waitFor(`(document.getElementById('log').textContent.match(/cast: \\d+ presences from \\d+ facts[^\\n]*/) || [])[0]`, 60000);
check('A5 the Scene toggle shows my room and its cast, with no Nell', scene && castLine && !/\bNell\b/.test(castLine), castLine);
await click('#ask-return');
const back = await waitFor(`getComputedStyle(document.getElementById('ask')).display !== 'none'`, 5000);
check('A5 the return button brings the conversation back', back);

const errors = consoleLines.filter((l) => !/127\.0\.0\.1:1|ERR_CONNECTION_REFUSED|Failed to load resource|net::ERR|404|401/.test(l));
check('no console errors or exceptions', errors.length === 0, errors.join(' | '));
const ok = results.every((r) => r.ok);
mkdirSync(dirname(reportPath), { recursive: true });
writeFileSync(reportPath, JSON.stringify({ ok, at: new Date().toISOString(), chrome, checks: results }, null, 1));
console.log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed · report ${reportPath}`);
process.exit(ok ? 0 : 1);
