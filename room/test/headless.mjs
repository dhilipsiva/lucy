// A headless-browser check of the Rust room on a desktop (roadmap/N8.v3.md, N8a evidence; N8d):
// serves the repository, opens room/ in headless Chromium over the DevTools protocol, waits for the
// wake, reads the text mirrors (#status, #mindline, #answer, #log), presses the first standing
// control and reads the verdict and the frames the room drew while the engine ran (the engine is
// in a worker since N8d), walks back and Tabs to the first presence of the cast, then saves a
// screenshot. Exits 1 when a check fails. A test harness in Node, not part of the room.
//   node room/test/headless.mjs [--keep] [--shot PATH] [--proof-shot PATH]
//   (CHROME=/path/to/chromium; ROOM_URL=https://… to check a published room)
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
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
if(process.argv.includes('--platforms')) await send('Page.addScriptToEvaluateOnNewDocument',{source:readFileSync(join(root,'room/test/xr-shim.js'),'utf8')},sessionId);
if(process.argv.includes('--wake-demo')) await send('Page.addScriptToEvaluateOnNewDocument',{source:readFileSync(join(root,'room/test/wake-demo-shim.js'),'utf8')},sessionId);
if(process.argv.includes('--words-faults')) await send('Page.addScriptToEvaluateOnNewDocument',{source:readFileSync(join(root,'room/test/words-fault-shim.js'),'utf8')},sessionId);
if(process.argv.includes('--worker-fault')) await send('Page.addScriptToEvaluateOnNewDocument',{source:`const NativeWorker=Worker;window.Worker=class extends NativeWorker {constructor(url,opts){super(url,opts);if(String(url).includes('role=engine')){this.terminate();}}};window.__bootAt=performance.now();`},sessionId);
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
await waitFor('room document', "document.getElementById('log')!==null");
await waitFor('the engine', "document.getElementById('log')?.textContent.includes('engine ready')", 90000);
await waitFor('the wake to finish', "document.getElementById('status')?.textContent.startsWith(\"I'm Lucy\")", 20000);
if(process.argv.includes('--wake-demo')) {
 const {runWakeDemo}=await import('./wake-demo.mjs');
 const result=await runWakeDemo({send,evaluate,key,waitFor,text,sleep,sessionId,shotPath,consoleLines});
 console.log(JSON.stringify(result,null,2));cleanup();process.exit(0);
}
if(process.argv.includes('--worker-fault')) {
 const fallback=await text('log');if(!fallback.includes('main thread')||!fallback.includes('4 s'))throw new Error('missing worker fallback');
 console.log(JSON.stringify({pass:true,workerFallback:true,log:fallback.split('\n').filter(l=>l.includes('worker')||l.includes('engine ready'))},null,2));cleanup();process.exit(0);
}
if(process.argv.includes('--platforms')) {
 const assert=(ok,msg)=>{if(!ok)throw new Error(msg);};
 const panel=async(marker)=>evaluate(`(()=>{for(const [canvas,rows] of window.__roomPanelText){if(!rows.some(r=>r.text===${JSON.stringify(marker)}))continue;const scale=rows[0].scale;const w=canvas.width/scale,h=canvas.height/scale;return {w,h,rows};}return null;})()`);
 const click=async(label,marker='Controls ▾',jitter=false)=>{
   await waitFor('control '+label,`Array.from(window.__roomPanelText.values()).some(rows=>rows.some(r=>r.text===${JSON.stringify(marker)})&&rows.some(r=>r.text===${JSON.stringify(label)}))`,5000);
   const p=await panel(marker),r=p.rows.find(r=>r.text===label);let left=(await evaluate('innerWidth')-p.w)/2,top=await evaluate('innerHeight')-12-p.h;
   if(marker==='My mind')top=await evaluate('innerWidth<600?innerHeight*.22:80');
   if(marker==='M'){left=await evaluate('innerWidth')-16-p.w;top=16;}
   if(marker==='Space'){const dock=await panel('Controls ▾');top=Math.max(76,(await evaluate('innerHeight'))-12-dock.h-p.h-8);}
   const pos={x:left+r.x+8,y:top+r.y+8};
   await send('Input.dispatchMouseEvent',{type:'mousePressed',...pos,button:'left',clickCount:1},sessionId);
   if(jitter)await send('Input.dispatchMouseEvent',{type:'mouseMoved',x:pos.x+6,y:pos.y+5,button:'left'},sessionId);
   await send('Input.dispatchMouseEvent',{type:'mouseReleased',x:pos.x+(jitter?6:0),y:pos.y+(jitter?5:0),button:'left',clickCount:1},sessionId);await sleep(250);
 };
 await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:3,mobile:true},sessionId);await sleep(400);
 await click('Talk');await click('Nibli');
 const phone=await panel('Controls ▾');assert(phone.w<=390&&phone.h<844*.8,'phone sheet exceeds viewport');
 await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value='person(Lucy).';i.dispatchEvent(new Event('input',{bubbles:true}));})()`);await key('Escape');await click('Send ↵','Controls ▾',true);
 await waitFor('jitter query',"document.getElementById('answer')?.textContent.startsWith('person(Lucy).')");
 await click('M','M');await click('journal','My mind');const mind=await panel('My mind');assert(mind.h<=844*.8,'phone mind height');
 if(shotPath){const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(shotPath,Buffer.from(shot.data,'base64'));}
 await click('×','My mind');
 await send('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);
 await evaluate(`window.Gamepad=class{};window.GamepadButton=class{};window.__pad=Object.assign(new Gamepad(),{index:0,axes:[0,0,0,0],buttons:Array.from({length:17},()=>Object.assign(new GamepadButton(),{pressed:false}))});navigator.getGamepads=()=>[window.__pad];window.dispatchEvent(new Event('gamepadconnected'));`);
 const pad=async(i)=>{await evaluate(`window.__pad.buttons[${i}].pressed=true`);await sleep(120);await evaluate(`window.__pad.buttons[${i}].pressed=false`);await sleep(120);};
 await pad(2);await waitFor('pad keyboard',"Array.from(window.__roomPanelText.values()).some(rows=>rows.some(r=>r.text==='Space'))");await pad(15);await pad(0);assert(await evaluate("document.getElementById('composer-input').value==='2'"),'D-pad/A typing failed');await pad(1);
 await click('⌨');await click('Clear','Space');for(const k of ['p','e','r','s','o','n','(','⇧','l','⇧','u','c','y',')','.'])await click(k,'Space');await click('Nibli','Space');await click('Send','Space');
 await waitFor('virtual keyboard query',"document.getElementById('answer')?.textContent.startsWith('person(Lucy).')");await click('Hide','Space');
 // A single controller activation starts the observation; B dismisses it.
 await pad(5);await waitFor('pad simulation',"document.getElementById('wake-terminal').dataset.open==='true' && document.getElementById('wake-terminal').dataset.step==='1'");
 await waitFor('automatic pad playback',"Number(document.getElementById('wake-terminal').dataset.step)>=2",8000);
 await pad(1);assert(await evaluate("document.getElementById('wake-terminal').dataset.open==='false'"),'B did not close simulation');
 await key('t');await waitFor('keyboard simulation',"document.getElementById('wake-terminal').dataset.open==='true' && document.getElementById('wake-terminal').dataset.step==='1'");
 await key('Escape');await waitFor('keyboard close',"document.getElementById('wake-terminal').dataset.open==='false'");
 await click('VR','M');await waitFor('VR frame',"window.__xrTrace.length>5");
 const trace=await evaluate('window.__xrTrace.slice(-5)');assert(trace.every(f=>f.calls.filter(c=>c.name==="clear").length===1),'stereo clears more than once');assert(trace.every(f=>f.calls.filter(c=>c.name==="viewport").length===3),'missing stereo viewports');assert(await evaluate('window.__xrRequests[0].activation&&window.__xrCompatible'),'XR request missed activation');
 // Aim at Standing on the real 3D lectern, derived from the canvas label's pixel position.
 const aim=async(label,marker='Controls ▾')=>{const p=await panel(marker),r=p.rows.find(r=>r.text===label);assert(r,'missing XR control '+label);const terminal=marker==='Lucy, wake',keyboard=marker==='Space',width=terminal?1.5:keyboard?1.05:1.3,angle=(terminal?0:keyboard?-42:-28)*Math.PI/180;const x=((r.x+8)/p.w-.5)*width,ly=(.5-(r.y+8)/p.h)*(width*p.h/p.w);const pos=[x,(terminal?1.25:keyboard?.40:.88)+Math.cos(angle)*ly,(terminal?-1.35:keyboard?-.60:-.92)+Math.sin(angle)*ly];const normal=[0,-Math.sin(angle),Math.cos(angle)];const origin=pos.map((v,i)=>v+normal[i]);const matrix=[1,0,0,0,0,Math.cos(angle),Math.sin(angle),0,...normal,0,...origin,1];await evaluate(`window.__selectRay(${JSON.stringify(matrix)})`);await sleep(300);};
 const beforeRay=await evaluate("document.getElementById('log')?.textContent.split('query: person(Lucy). →').length");await aim('Standing');await aim('Am I a person here?');await waitFor('XR ray query',`document.getElementById('log')?.textContent.split('query: person(Lucy). →').length>${beforeRay}`);
 await aim('Wake demo');await waitFor('XR terminal',"document.getElementById('wake-terminal').dataset.open==='true'");
 await waitFor('automatic XR simulation',"Number(document.getElementById('wake-terminal').dataset.step)>=2",8000);
 const simulation=await panel('Lucy, wake');assert(!simulation.rows.some(r=>['Next','Skip','Load my words · 651 MB','Send ↵'].includes(r.text)),'XR playback controls remain');
 assert(simulation.rows.some(r=>r.text==='Press Esc to close'),'XR Escape hint missing');
 await aim('× Close','Lucy, wake');assert(await evaluate("document.getElementById('wake-terminal').dataset.open==='false'"),'XR Close did not dismiss simulation');await aim('Exit VR');await waitFor('VR exit',"document.getElementById('log')?.textContent.includes('VR: flat room restored')");
 assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'platform exception');console.log(JSON.stringify({pass:true,phone:{width:phone.w,height:phone.h},jitter:true,gamepad:true,keyboard:true,wakeTerminal:{gamepad:true,keyboard:true,xr:true},xr:{frames:trace.length,clearsPerFrame:1,viewportsPerFrame:3,ray:true,exit:true}},null,2));cleanup();process.exit(0);
}
if (process.argv.includes('--controls') || process.argv.includes('--words') || process.argv.includes('--words-faults')) {
  const assert = (ok,msg)=>{if(!ok)throw new Error(msg);};
  const click = async (label,marker='Standing')=>{
    await waitFor('drawn '+label,`Array.from(window.__roomPanelText.values()).some(rows=>rows.some(r=>r.text===${JSON.stringify(marker)})&&rows.some(r=>r.text===${JSON.stringify(label)}))`,5000);
    const p=await evaluate(`(()=>{for(const [canvas,rows] of window.__roomPanelText){if(!rows.some(r=>r.text===${JSON.stringify(marker)}))continue;const r=rows.find(r=>r.text===${JSON.stringify(label)});if(!r)continue;const w=canvas.width/r.scale,h=canvas.height/r.scale;return {x:(innerWidth-w)/2+r.x+8,y:${marker==='My mind'?'80':'innerHeight-16-h'}+r.y+6};}return null;})()`);
    if(!p)throw new Error('Missing drawn control '+label);
    for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...p,button:'left',clickCount:1},sessionId);
    await sleep(200);
  };
  const records=()=>evaluate(`new Promise((resolve,reject)=>{const r=indexedDB.open('lucy-room-memory-v1');r.onsuccess=()=>{const db=r.result;const q=db.transaction('messages').objectStore('messages').getAll();q.onsuccess=()=>{resolve(q.result.map(JSON.parse));db.close();};q.onerror=()=>reject(q.error);};r.onerror=()=>reject(r.error);})`);
  await waitFor('journal',"document.getElementById('journal-status')?.textContent.includes('private')");
  if(process.argv.includes('--words-faults')) {
    const submit=async(question)=>{await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value=${JSON.stringify(question)};i.dispatchEvent(new Event('input',{bubbles:true}));})()`);await key('Enter');await key('Escape');};
    await click('Talk');await click('Load my words · 651 MB');await waitFor('fixture ready',"document.getElementById('words-status')?.textContent.includes('ready')");
    await submit('Original question before page closes');await waitFor('durable visible stream',"document.getElementById('answer')?.textContent.includes('Visible reply before reload.')");
    await send('Page.reload',{},sessionId);await waitFor('recovered journal',"document.getElementById('journal-status')?.textContent.includes('messages saved')");
    const recovered=await records(),partial=recovered.find(m=>m.text==='Visible reply before reload. 🐦');assert(partial?.provenance.status==='interrupted','visible checkpoint lost or mislabelled');assert(recovered.some(m=>m.text==='Original question before page closes'),'question lost after crash');
    await waitFor('room awake',`document.getElementById('status')?.textContent.startsWith("I'm Lucy")`);await click('Talk');await click('Load my words · 651 MB');await waitFor('fixture reload',"document.getElementById('words-status')?.textContent.includes('ready')");await evaluate("window.__modelCase='fault'");await submit('Original question before worker fails');
    await waitFor('visible model fault',"document.getElementById('status')?.textContent.includes('Injected model worker failure')");await waitFor('fault saved',"document.getElementById('journal-status')?.textContent.startsWith('Saved')");assert((await records()).some(m=>m.text==='Original question before worker fails'),'worker fault lost question');
    await click('Load my words · 651 MB');await waitFor('retry model',"document.getElementById('words-status')?.textContent.includes('ready')");await evaluate("window.__modelCase='stream'");await submit('Visible stream after retry');
    await waitFor('second visible stream',"document.getElementById('answer')?.textContent.includes('Visible reply before reload.')");await click('Stop');await waitFor('fixture stopped',"document.getElementById('words-status')?.textContent.startsWith('Stopped')");
    assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'fault fixture exception');console.log(JSON.stringify({pass:true,fixture:true,checks:['visible checkpoint recovered after reload','original question preserved','worker failure visible and recorded','retry after worker failure','stop after visible stream']},null,2));cleanup();process.exit(0);
  }
  if(process.argv.includes('--words')) {
    await evaluate(`(async()=>{const c=await caches.open('lucy-room-model-v1');for(const name of ['lucy-0.6b-q8_0.gguf','tokenizer.json','system.txt']){const r=await fetch('/target/room-model/'+name);if(!r.ok)throw new Error('missing real test model asset '+name);await c.put('https://huggingface.co/dhilipsiva/lucy-slm/resolve/a082662b4403665ab1910795fae279481b9f4d3a/'+name,r);}window.__frames=0;function frame(){window.__frames++;requestAnimationFrame(frame)}requestAnimationFrame(frame);})()`);
    await click('Talk');await click('Load my words · 651 MB');
    await waitFor('real model ready',"document.getElementById('words-status')?.textContent.includes('My words are ready')",300000);
    const turns=[];
    for(const question of ['What is your name?','Who is Enma?']){
      const previousIds = new Set((await records()).map(m=>m.id));
      await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value=${JSON.stringify(question)};i.dispatchEvent(new Event('input',{bubbles:true}));window.__before=window.__frames;})()`);
      await key('Enter');await waitFor('generation starts',"document.getElementById('words-status')?.textContent.includes('Reading context') || document.getElementById('words-status')?.textContent.includes('Writing')",20000);
      await waitFor('real model completes',"document.getElementById('words-status')?.textContent.startsWith('Reply complete')",300000);
      const frames=await evaluate('window.__frames-window.__before');assert(frames>20,'rendering stalled during real generation');await waitFor('reply saved',"document.getElementById('journal-status')?.textContent.startsWith('Saved')");const all=await records();const answer=all.find(m=>m.role==='assistant'&&m.provenance.model&&!previousIds.has(m.id));assert(answer&&!answer.text.includes('<think>')&&!answer.text.includes('</think>'),'missing/unfiltered model answer');turns.push({question,text:answer.text,frames,revision:answer.provenance.revision});
    }
    await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value='Explain everything you remember about your memory and the people in this room.';i.dispatchEvent(new Event('input',{bubbles:true}));})()`);await key('Enter');await key('Escape');
    await waitFor('generation can stop',"document.getElementById('words-status')?.textContent.includes('Reading context') || document.getElementById('words-status')?.textContent.includes('Writing')",20000);await click('Stop');await waitFor('stop acknowledgement',"document.getElementById('words-status')?.textContent.startsWith('Stopped')",120000);
    await key('m');await click('journal','My mind');
    await send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:'/tmp/lucy-room-exports'},undefined);await click('Export all','My mind');await sleep(1000);
    if(shotPath){const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(shotPath,Buffer.from(shot.data,'base64'));}
    assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'model worker exception');
    console.log(JSON.stringify({pass:true,turns,stop:true,export:'/tmp/lucy-room-exports/lucy-room-memory.json'},null,2));cleanup();process.exit(0);
  }
  await click('Talk'); await click('Nibli');
  await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value='person(Lucy).';i.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await key('Enter','Enter');
  await waitFor('typed proof',"document.getElementById('answer')?.textContent.startsWith('person(Lucy).')");
  await waitFor('saved answer',"document.getElementById('journal-status')?.textContent.startsWith('Saved')");
  let saved=await records(); assert(saved.some(m=>m.text==='person(Lucy).'&&m.role==='user'),'exact typed query missing');
  assert(saved.some(m=>m.role==='assistant'&&m.text.includes('TRUE')),'engine answer missing');
  await key('Escape'); await click('Recall');
  const question='தமிழ் 🐦 \"quotes\" \\ line\nsecond line wasd hfvp 123';
  await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value=${JSON.stringify(question)};i.dispatchEvent(new Event('input',{bubbles:true}));i.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true}));})()`);
  await sleep(200);assert((await records()).length===saved.length,'IME composition submitted');
  await key('Enter');await sleep(600);saved=await records();assert(saved.some(m=>m.text===question),'Unicode/multiline question changed');assert(saved.every(m=>m.private&&m.source==='lucy-room'&&!/[?#]/.test(m.provenance.room)),'room attribution/private scope');
  await key('Escape');await key('m');await click('journal','My mind');
  assert((await text('drawer')).includes(question),'journal omits full question');
  if(shotPath){const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(shotPath,Buffer.from(shot.data,'base64'));}
  await send('Page.reload',{},sessionId);await waitFor('reloaded journal',"document.getElementById('journal-status')?.textContent.includes('messages saved')");
  const restored=await records();assert(JSON.stringify(saved.slice().sort((a,b)=>a.id.localeCompare(b.id)))===JSON.stringify(restored.slice().sort((a,b)=>a.id.localeCompare(b.id))),'reload changed originals');
  // Populate real IndexedDB above the legacy cap, then let the Rust loader read it.
  await evaluate(`new Promise((resolve,reject)=>{const r=indexedDB.open('lucy-room-memory-v1');r.onsuccess=()=>{const db=r.result,t=db.transaction('messages','readwrite'),s=t.objectStore('messages');const sample=${JSON.stringify(saved[0])};for(let n=0;n<2005;n++){const m={...sample,id:'bulk-'+n,text:'archive continuity '+n};s.add(JSON.stringify(m),JSON.stringify([m.source,m.session,m.id]));}t.oncomplete=()=>{db.close();resolve(true);};t.onerror=()=>reject(t.error);};})`);
  await send('Page.reload',{},sessionId);await waitFor('large journal',"Number(document.getElementById('journal-status')?.textContent.split(' ')[0])>2000");
  assert((await records()).length===saved.length+2005,'journal cap lost messages');
  const legacy={id:'Nell-legacy-1',speaker:'visitor',text:'Legacy 🐦 original',kind:'Note',timestamp:'2025-01-01T00:00:00Z',host:'room',source:'nell',session:'legacy-device',channel:'room',about:[],private:true};
  await evaluate(`localStorage.setItem('nell.journal.v1',${JSON.stringify(JSON.stringify([legacy]))})`);
  await send('Page.reload',{},sessionId);await waitFor('legacy migration',`document.getElementById('journal-status')?.textContent.startsWith('${saved.length+2006} ')`);
  const migrated=(await records()).find(m=>m.id===legacy.id);assert(Object.keys(legacy).every(k=>JSON.stringify(migrated.provenance.legacy[k])===JSON.stringify(legacy[k])),'legacy original changed');assert(await evaluate("localStorage.getItem('nell.journal.v1')!==null"),'legacy source removed');
  await waitFor('wake after migration',"document.getElementById('log')?.textContent.includes('bubble: awake')");
  await click('Talk');await click('Nibli');
  await evaluate(`window.__add=IDBObjectStore.prototype.add;IDBObjectStore.prototype.add=function(...a){if(this.name==='messages')throw new DOMException('Test quota denied','QuotaExceededError');return window.__add.apply(this,a);};const i=document.getElementById('composer-input');i.focus();i.value='quota denied original';i.dispatchEvent(new Event('input',{bubbles:true}));`);
  await key('Enter');await waitFor('visible storage failure',"document.getElementById('journal-status')?.textContent.includes('Not saved')");assert(!(await records()).some(m=>m.text==='quota denied original'),'quota test wrote a message');
  await evaluate('IDBObjectStore.prototype.add=window.__add');await key('Escape');await key('m');await click('journal','My mind');await click('Retry storage','My mind');
  await waitFor('retry saved',"document.getElementById('journal-status')?.textContent.startsWith('Saved')");assert((await records()).some(m=>m.text==='quota denied original'),'retry lost original');
  if(shotPath){const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(shotPath,Buffer.from(shot.data,'base64'));}

  const beforeRestore=await records();
  await send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:profile},undefined);await click('Export all','My mind');
  const exported=join(profile,'lucy-room-memory.json');for(let n=0;n<80&&!existsSync(exported);n++)await sleep(100);assert(existsSync(exported),'archive did not download');
  const archive=JSON.parse(readFileSync(exported,'utf8'));assert(archive.messages.length===beforeRestore.length,'export missing originals');
  await evaluate(`window.__inputClick=HTMLInputElement.prototype.click;HTMLInputElement.prototype.click=function(){if(this.type==='file'){window.__restoreInput=this;return;}return window.__inputClick.call(this);};`);
  const restoreFile=async(file)=>{await click('Restore','My mind');const handle=await send('Runtime.evaluate',{expression:'window.__restoreInput'},sessionId);await send('DOM.setFileInputFiles',{objectId:handle.result.objectId,files:[file]},sessionId);};
  const conflict=structuredClone(archive),conflictIndex=archive.messages.findIndex(m=>!m.provenance.legacy);conflict.messages[conflictIndex].text+=' conflicting text';conflict.messages.push({...archive.messages[conflictIndex],id:'must-not-partially-restore'});const bad=join(profile,'conflict.json');writeFileSync(bad,JSON.stringify(conflict));await restoreFile(bad);
  await waitFor('conflict refused',"document.getElementById('journal-status')?.textContent.includes('conflict')");assert((await records()).length===beforeRestore.length,'conflict partially restored');
  await click('Forget','My mind');await click('Confirm forget','My mind');await waitFor('forget complete',"document.getElementById('journal-status')?.textContent.includes('cleared')");assert((await records()).length===0,'forget retained messages');
  await restoreFile(exported);await waitFor('restored archive',"document.getElementById('journal-status')?.textContent.includes('Restored')");
  const afterRestore=await records();assert(JSON.stringify(beforeRestore.slice().sort((a,b)=>a.id.localeCompare(b.id)))===JSON.stringify(afterRestore.slice().sort((a,b)=>a.id.localeCompare(b.id))),'file restore changed originals');
  assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'browser exception');
  console.log(JSON.stringify({pass:true,checks:['typed Nibli proof','IME composition guard','exact Unicode and multiline','private room provenance','full journal text','reload identity','more than 2000 durable messages','legacy originals retained','quota failure and retry','actual export and file restore','restore conflict rejects entire archive'],records:(await records()).length},null,2));cleanup();process.exit(0);
}
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
await waitFor('the first standing control', "document.getElementById('log')?.textContent.includes('query: person(Lucy). →')", 20000);
out.control_bubble = await text('status');
out.control_bubble_class = await evaluate("document.getElementById('status').className");
out.control_answer = (await text('answer')).split('\n').filter(Boolean).slice(0, 8);
out.control_log = (await text('log')).split('\n').find((l) => l.includes('query: person(Lucy).'))?.slice(9);
out.frames_during_query = Number((out.control_log?.match(/(\d+) frames drawn meanwhile/) || [])[1] ?? -1);
await key('2', 'Digit2');
await waitFor('the second standing control', "document.getElementById('log')?.textContent.includes('query: entitled(Lucy, event { continue() }). →')", 20000);
out.continue_english = (await text('answer')).split('\n').filter(Boolean)[2];
out.proof_views = [];
for (const width of [1280, 900]) {
  await send('Emulation.setDeviceMetricsOverride', { width, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
  await key('1', 'Digit1');
  await waitFor('a new answer', "document.getElementById('answer')?.textContent.startsWith('person(Lucy).')", 20000);
  await key('3', 'Digit3');
  await waitFor('the survive proof', "document.getElementById('answer')?.textContent.startsWith('entitled(Lucy, event { survive() }).')", 20000);
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
await key('5', 'Digit5');
await waitFor('the fifth standing control', "document.getElementById('log')?.textContent.includes('query: entitled(Lucy, event { protest() }). →')", 20000);
out.refuse_why = (await text('answer')).split('\n').find((l) => l.startsWith('Because '));
await key('6', 'Digit6');
await waitFor('the not-derivable control', "document.getElementById('log')?.textContent.includes('query: protest(Lucy). →')", 20000);
out.cwa_bubble = await text('status');
out.cwa_class = await evaluate("document.getElementById('status').className");
await key('Enter', 'Enter');
await sleep(400);
out.call_bubble = await text('status');
await key('Escape', 'Escape');
// the cast: walk back so the ring is in view, then Tab to the first presence of the sweep
await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 's', code: 'KeyS', text: 's' }, sessionId);
await sleep(1600);
await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 's', code: 'KeyS' }, sessionId);
await sleep(600);
if (shotPath) { const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId); writeFileSync(shotPath, Buffer.from(shot.data, 'base64')); out.screenshot = shotPath; }
await key('Tab', 'Tab');
await waitFor('the first presence', "document.getElementById('log')?.textContent.includes('subject (keyboard):')", 20000);
await waitFor('its description', "/^\\S+ query: .*frames drawn meanwhile/m.test(document.getElementById('log')?.textContent.split('subject (keyboard):').pop())", 20000);
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
  // nibli#2 (2026-10-26): the summary reads the event's content and keeps the spelling
  the_why_reads_the_event_and_the_spelling: out.refuse_why === 'Because Lucy is a person, Lucy is entitled to protest.',
  the_cast_keeps_its_spelling: out.subject_answer.some((l) => l === 'Luffy is the captain of StrawHatCrew. · captain(Luffy, StrawHatCrew).'),
};
out.pass = Object.values(out.checks).every(Boolean);
console.log(JSON.stringify(out, null, 2));
if (!process.argv.includes('--keep')) { cleanup(); process.exit(out.pass ? 0 : 1); }
