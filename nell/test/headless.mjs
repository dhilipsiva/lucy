// A headless-browser check of the room on a desktop (roadmap/N2b.md, evidence): serves the
// repository, opens nell/ in headless Chrome over the DevTools protocol, waits for the mind,
// the public memory and the hashed index, asks in words with no model loaded, recalls by
// meaning, and prints what the bubble and the log said. Needs google-chrome and the network;
// run by hand, not in CI:  node nell/test/headless.mjs [--keep]
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PORT = 8788, CDP = 9333;
const chrome = process.env.CHROME || 'google-chrome';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const server = spawn('python3', ['-m', 'http.server', String(PORT), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
const profile = mkdtempSync(join(tmpdir(), 'nell-headless-'));
const browser = spawn(chrome, ['--headless=new', `--remote-debugging-port=${CDP}`, `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--disable-gpu-sandbox', 'about:blank'], { stdio: 'ignore' });
const cleanup = () => { try { browser.kill(); } catch {} try { server.kill(); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
process.on('exit', cleanup);

let wsUrl = null;
for (let i = 0; i < 50 && !wsUrl; i++) {
  await sleep(200);
  try { const r = await fetch(`http://127.0.0.1:${CDP}/json/version`); wsUrl = (await r.json()).webSocketDebuggerUrl; } catch { /* not yet */ }
}
if (!wsUrl) { console.error('chrome did not open its debugging port'); process.exit(2); }
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
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description || '')); return r.result.value; };
const logText = () => evaluate("document.getElementById('log').textContent");
const waitFor = async (what, test, ms = 60000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (await evaluate(test)) return; await sleep(300); }
  throw new Error(`timed out waiting for ${what}\n--- log ---\n${await logText()}\n--- console ---\n${consoleLines.join('\n')}`);
};
const out = {};
await send('Page.navigate', { url: `http://127.0.0.1:${PORT}/nell/` }, sessionId);
await waitFor('the engine', "document.getElementById('log').textContent.includes('engine ready')");
await waitFor('the public memory and the hashed index', "document.getElementById('log').textContent.includes('index: hashed space')", 90000);
const log1 = await logText();
out.sources = log1.split('\n').filter((l) => /^\S+ (constitution|memory): /.test(l)).map((l) => l.slice(9));
out.index = log1.split('\n').find((l) => l.includes('index: hashed space'))?.slice(9);
out.controls = log1.split('\n').find((l) => l.includes('controls:'))?.slice(9);
out.standing = await evaluate("document.getElementById('standing').textContent");
out.bubble_after_load = await evaluate("document.getElementById('status').textContent");

await evaluate("document.getElementById('sayQuery').value = 'Who are Enma and Pity?'; document.getElementById('say').requestSubmit(); true");
await waitFor('the words box (retrieval alone)', "document.getElementById('status').textContent.includes('My words are not loaded')");
out.words_bubble = await evaluate("document.getElementById('status').textContent");
out.words_fed = (await evaluate("document.getElementById('answer').textContent")).split('\n').slice(0, 12);

await evaluate("document.getElementById('recallQuery').value = 'conures'; document.getElementById('recall').requestSubmit(); true");
await waitFor('recall by the hashed space', "document.getElementById('log').textContent.includes('recall: conures')");
out.hashed_recall_bubble = await evaluate("document.getElementById('status').textContent");

await evaluate("document.getElementById('recallQuery').value = 'xq zzq qqxv wvvq'; document.getElementById('recall').requestSubmit(); true");
await waitFor('recall of nonsense', "document.getElementById('log').textContent.includes('recall: xq zzq')");
out.nonsense_bubble = await evaluate("document.getElementById('status').textContent");

// The neural index: the pinned library and model are fetched (about 24 MB, once), every item embedded, then "the parrot" must reach the Sun Conure by meaning.
if (!process.argv.includes('--no-neural')) {
  await evaluate("[...document.querySelectorAll('#chips button')].find((b) => b.textContent.startsWith('Recall by meaning: build')).click(); true");
  await waitFor('the neural index', "/index: Xenova|index: worker error|index: .*(error|could not)/i.test(document.getElementById('log').textContent)", 300000);
  out.neural_index = (await logText()).split('\n').filter((l) => /index: (Xenova|worker|.*(error|could not))/i.test(l)).map((l) => l.slice(9));
  out.neural_words_line = await evaluate("document.getElementById('words').textContent");
  await evaluate("document.getElementById('recallQuery').value = 'the parrot'; document.getElementById('recall').requestSubmit(); true");
  await waitFor('recall by meaning', "document.getElementById('log').textContent.includes('recall: the parrot')", 60000);
  out.neural_recall_bubble = await evaluate("document.getElementById('status').textContent");
  out.neural_recall_answer = (await evaluate("document.getElementById('answer').textContent")).split('\n').filter(Boolean).slice(0, 8);
  await evaluate("document.getElementById('recallQuery').value = 'xq zzq qqxv wvvq'; document.getElementById('recall').requestSubmit(); true");
  await waitFor('recall of nonsense, neural', "document.getElementById('log').textContent.split('recall: xq zzq').length > 2", 60000);
  out.neural_nonsense_bubble = await evaluate("document.getElementById('status').textContent");
}

await evaluate("document.getElementById('query').value = 'person(Lucy).'; document.getElementById('ask').requestSubmit(); true");
await waitFor('a Nibli answer', "document.getElementById('log').textContent.includes('query: person(Lucy). →')");
out.nibli_bubble = await evaluate("document.getElementById('status').textContent");
out.bubble_class = await evaluate("document.getElementById('status').className");
out.journal = await evaluate("document.getElementById('journal').textContent.split('\\n')[0]");

// A second load must reuse the stored vectors.
await send('Page.navigate', { url: `http://127.0.0.1:${PORT}/nell/` }, sessionId);
await waitFor('the second load', "document.getElementById('log').textContent.includes('index: hashed space')", 90000);
out.index_second_load = (await logText()).split('\n').find((l) => l.includes('index: hashed space'))?.slice(9);
out.console = consoleLines.filter((l) => !/^(log|info|debug):/.test(l)).slice(0, 20);
console.log(JSON.stringify(out, null, 2));
if (!process.argv.includes('--keep')) { cleanup(); process.exit(0); }
