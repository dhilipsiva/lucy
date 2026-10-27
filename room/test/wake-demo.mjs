import { writeFileSync, readFileSync, existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

export async function runWakeDemo({send, evaluate, key, waitFor, text, sleep, sessionId, shotPath, consoleLines, real}) {
  const assert = (v, message) => { if (!v) throw new Error(message); };
  const state = () => evaluate("({...document.getElementById('wake-terminal').dataset})");
  const records = () => evaluate(`new Promise((resolve,reject)=>{const r=indexedDB.open('lucy-room-memory-v1');r.onsuccess=()=>{const db=r.result;const q=db.transaction('messages').objectStore('messages').getAll();q.onsuccess=()=>{resolve(q.result.map(JSON.parse));db.close();};q.onerror=()=>reject(q.error);};r.onerror=()=>reject(r.error);})`);
  const panel = (marker='Lucy, wake') => evaluate(`(()=>{for(const [canvas,rows] of window.__roomPanelText){if(!rows.some(r=>r.text===${JSON.stringify(marker)}))continue;const scale=rows[0].scale;return {w:canvas.width/scale,h:canvas.height/scale,rows};}return null;})()`);
  const click = async (label, marker='Lucy, wake') => {
    await waitFor('drawn '+label, `Array.from(window.__roomPanelText.values()).some(rows=>rows.some(r=>r.text===${JSON.stringify(marker)})&&rows.some(r=>r.text===${JSON.stringify(label)}))`, 5000);
    const p = await panel(marker), row = p.rows.find(r=>r.text===label);
    const size = await evaluate('({w:innerWidth,h:innerHeight})');
    const top = marker==='Lucy, wake' ? (size.h-p.h)/2 : marker==='My mind' ? (size.w<600?size.h*.22:80) : size.h-(size.w<600?12:16)-p.h;
    const point = {x:(size.w-p.w)/2+row.x+8,y:top+row.y+6};
    for(const type of ['mousePressed','mouseReleased']) await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1},sessionId);
    await sleep(150);
  };
  const draft = s => evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value=${JSON.stringify(s)};i.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  const submit = async s => {
    await draft(s);await key('Enter');await key('Escape');
    if (!real && s !== 'Storage retry original') await waitFor('new generation dispatched',`window.__wakeRequests.some(r=>r.kind==='generate' && r.question===${JSON.stringify(s)})`);
    if (real) await waitFor('new generation starts',"/Reading context|Writing/.test(document.getElementById('words-status').textContent)",20000);
  };
  const open = async()=>{await key('t');await waitFor('terminal opens',"document.getElementById('wake-terminal').dataset.open==='true'");};
  const screenshot = async path => {const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(path,Buffer.from(shot.data,'base64'));};
  await waitFor('private journal',"document.getElementById('journal-status').textContent.includes('private')");
  if(shotPath) await screenshot(shotPath.replace('.png','-room.png'));
  // The freestanding screen is in this part of the default 1280×800 view.
  for(const type of ['mousePressed','mouseReleased']) await send('Input.dispatchMouseEvent',{type,x:380,y:190,button:'left',clickCount:1},sessionId);
  await waitFor('scene screen opens',"document.getElementById('wake-terminal').dataset.open==='true'");
  await click('×');await click('Wake demo','Standing');
  await waitFor('Controls entry opens',"document.getElementById('wake-terminal').dataset.open==='true'");
  await click('×');
  await open();
  await click('Start');await click('Pause');
  const paused=await state();await sleep(3500);assert((await state()).state===paused.state,'paused replay advanced');
  await click('Next');const advanced=await state();assert(advanced.state!==paused.state,'Next did not advance');
  await click('×');await open();assert((await state()).state===advanced.state,'reopen reset replay');
  await click('Skip');assert((await state()).state==='Demonstration complete','Skip did not finish');
  assert((await text('wake-terminal')).includes('Lucy: cached'),'cached peer example missing');
  assert((await records()).length===0,'scripted transcript entered journal');
  if(!real) assert(await evaluate('window.__wakeWorkers===0 && window.__wakeRequests.length===0'),'model started during replay');
  await click('Replay');assert(!(await text('wake-terminal')).includes('Permission granted'),'Replay retained finished steps');
  await click('Skip');
  await send('Emulation.setDeviceMetricsOverride',{width:320,height:700,deviceScaleFactor:2,mobile:true},sessionId);await sleep(250);
  let p=await panel();assert(p.w<=320&&p.h<=700,'terminal exceeds phone');
  assert(p.rows.every(r=>r.x>=0&&r.y>=0&&r.x+r.width<=p.w+1&&r.y+12<=p.h+1),'terminal text clipped on phone');
  const tailRows=JSON.stringify((await panel()).rows);await click('↑ Older');assert(JSON.stringify((await panel()).rows)!==tailRows,'scroll did not move');
  for(let i=0;i<40 && !(await panel()).rows.some(r=>r.text==='DEMONSTRATION · a CLI-agent wake');i++) await click('↑ Older');
  assert((await panel()).rows.some(r=>r.text==='DEMONSTRATION · a CLI-agent wake'),'cannot reach transcript start');
  await click('End');
  if(shotPath) await screenshot(shotPath.replace('.png','-phone.png'));
  await send('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);await sleep(200);
  // A real anchor uses the canonical URL; intercept navigation, never open another site in tests.
  await evaluate(`window.__oldClick=HTMLAnchorElement.prototype.click;HTMLAnchorElement.prototype.click=function(){window.__openedRoom=this.href;}`);
  await click('https://dhilipsiva.dev/lucy/room/');
  assert(await evaluate("window.__openedRoom==='https://dhilipsiva.dev/lucy/room/'"),'wrong room link');
  await evaluate('HTMLAnchorElement.prototype.click=window.__oldClick');
  if(real) {
    await evaluate(`(async()=>{const c=await caches.open('lucy-room-model-v1');for(const name of ['lucy-0.6b-q8_0.gguf','tokenizer.json','system.txt']){const r=await fetch('/target/room-model/'+name);if(!r.ok)throw new Error('missing real model asset '+name);await c.put('https://huggingface.co/dhilipsiva/lucy-slm/resolve/a082662b4403665ab1910795fae279481b9f4d3a/'+name,r);}window.__frames=0;function frame(){window.__frames++;requestAnimationFrame(frame)}requestAnimationFrame(frame);})()`);
  } else {
    await evaluate("window.__wakeCase='loading'");await click('Load my words · 651 MB');
    await click('Cancel loading');assert((await text('words-status')).includes('Stopped loading'),'load cancellation failed');
    await evaluate("window.__wakeCase='complete'");
  }
  await click('Load my words · 651 MB');
  await waitFor('live handover',"document.getElementById('wake-terminal').dataset.state==='Live · on this device'",300000);
  await send('Emulation.setDeviceMetricsOverride',{width:320,height:700,deviceScaleFactor:2,mobile:true},sessionId);await sleep(250);
  p=await panel();assert(p.rows.every(r=>r.x>=0&&r.y>=0&&r.x+r.width<=p.w+1&&r.y+12<=p.h+1),'live terminal text clipped on phone');
  if(shotPath) await screenshot(shotPath.replace('.png','-phone-live.png'));
  await send('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);await sleep(200);
  const question=real?'What is your name?':'What happened?\nKeep this original: 🐦 தமிழ்';
  // IME Enter remains composition; Shift+Enter remains a newline.
  await draft(question);await evaluate("document.getElementById('composer-input').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true}))");
  assert(!(await records()).some(m=>m.text===question),'IME submitted prematurely');
  await key('Enter');await key('Escape');
  if(!real) await waitFor('first generation dispatched',`window.__wakeRequests.some(r=>r.kind==='generate' && r.question===${JSON.stringify(question)})`);
  if(real) await waitFor('first generation starts',"/Reading context|Writing/.test(document.getElementById('words-status').textContent)",20000);
  await waitFor('live reply',"document.getElementById('words-status').textContent.startsWith('Reply complete')",300000);
  await waitFor('saved reply',"document.getElementById('journal-status').textContent.startsWith('Saved')");
  let all=await records();let reply=all.find(m=>m.role==='assistant'&&m.provenance.model);
  assert(all.filter(m=>m.text===question).length===1,'question lost or duplicated');
  assert(reply&&reply.provenance.revision&&!reply.text.includes('<think>'),'real reply provenance/filter missing');
  assert((await text('wake-terminal')).includes(reply.text),'reply absent from terminal');
  if(!real) {
    const request=await evaluate("window.__wakeRequests.find(r=>r.kind==='generate')");
    assert(request.context.includes('illustrative')&&!request.history.includes('Permission granted'),'demo confused with actual history');
  }
  await draft('Shared draft');await key('Escape');await click('Open Talk');
  assert(await evaluate("document.getElementById('composer-input').value==='Shared draft'"),'draft lost switching to Talk');
  await submit(real?'Who is Enma?':'Follow-up from Talk');
  await waitFor('Talk reply',"document.getElementById('words-status').textContent.startsWith('Reply complete')",300000);
  await waitFor('Talk saved',"document.getElementById('journal-status').textContent.startsWith('Saved')");
  all=await records();assert(all.filter(m=>m.role==='user').length===2,'conversation duplicated');
  await open();assert((await text('wake-terminal')).includes(real?'Who is Enma?':'Follow-up from Talk'),'Talk missing in terminal');
  if(!real) {
    const requests=await evaluate("window.__wakeRequests.filter(r=>r.kind==='generate')");
    assert(requests[1].history.includes(question),'Talk lost terminal history');
    assert(await evaluate('window.__wakeWorkers===2'),'switching views created another worker');
    await evaluate("window.__wakeCase='stream'");await submit('Interrupt this visible reply');
    await waitFor('visible streaming',"document.getElementById('wake-terminal').textContent.includes('Visible local reply: Interrupt this visible reply')");
    await click('Stop');await waitFor('stop',"document.getElementById('words-status').textContent.startsWith('Stopped')");
    assert((await records()).some(m=>m.role==='assistant'&&m.provenance.status==='interrupted'),'interrupted reply not saved');
    await evaluate("window.__wakeCase='fault'");await submit('Worker failure original');
    await waitFor('model failure',"document.getElementById('words-status').textContent.includes('Demo fixture worker failure')");
    assert((await records()).some(m=>m.text==='Worker failure original'),'worker error lost original');
    await evaluate("window.__wakeCase='complete'");await click('Load my words · 651 MB');
    await waitFor('model retry',"document.getElementById('words-status').textContent.includes('ready')");
    // Persist before generation: a rejected write must not produce a model turn.
    await evaluate("window.__add=IDBObjectStore.prototype.add;IDBObjectStore.prototype.add=function(){throw new DOMException('Injected quota','QuotaExceededError')}");
    const before=await evaluate('window.__wakeRequests.length');await submit('Storage retry original');
    await waitFor('storage error',"document.getElementById('journal-status').textContent.includes('Not saved')");
    assert(await evaluate(`window.__wakeRequests.length===${before}`),'generation preceded persistence');
    await evaluate('IDBObjectStore.prototype.add=window.__add');await click('Retry save');
    await waitFor('retry response',"document.getElementById('words-status').textContent.startsWith('Reply complete')");
    await waitFor('retry saved',"document.getElementById('journal-status').textContent.startsWith('Saved')");
    assert((await records()).filter(m=>m.text==='Storage retry original').length===1,'retry duplicated original');
  }
  const saved=await records();await click('Replay');
  assert(!(await text('wake-terminal')).includes(question),'replay is hidden below the live conversation');
  await click('Skip');
  assert((await records()).length===saved.length,'replay changed shared records');
  await click('Continue · local model');assert((await state()).state==='Live · on this device','loaded model not reused');
  assert((await text('wake-terminal')).includes(question),'handover did not restore the shared conversation');
  if(shotPath) await screenshot(shotPath);
  await click('Open Talk');await key('m');await click('journal','My mind');
  const exportDir=mkdtempSync(join(tmpdir(),'lucy-wake-screen-exports-'));
  await send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:exportDir});await click('Export all','My mind');
  const exportPath=join(exportDir,'lucy-room-memory.json');
  for(let i=0;i<80&&!existsSync(exportPath);i++)await sleep(100);
  assert(existsSync(exportPath),'journal export did not download');
  const byId=(a,b)=>a.id.localeCompare(b.id);
  assert(isDeepStrictEqual(JSON.parse(readFileSync(exportPath,'utf8')).messages.toSorted(byId),saved.toSorted(byId)),'export changed or lost originals');
  await send('Page.reload',{},sessionId);await waitFor('reloaded journal',"document.getElementById('journal-status').textContent.includes('messages saved')");
  assert((await records()).length===saved.length,'reload lost conversation');
  assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'browser exception');
  return {pass:true,realModel:real,messages:saved.length,reply:reply.text,exportPath,checks:['scene and Controls entry','guided replay controls','script excluded from memory','phone replay and live layout','explicit model load',...(!real?['cancel and retry','stream/stop and failures']:['two real model turns']),'shared Talk history and draft','private originals and provenance','replay preserves conversation','exact export and reload']};
}
