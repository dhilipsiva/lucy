import { writeFileSync } from 'node:fs';

export async function runWakeDemo({send, evaluate, key, waitFor, text, sleep, sessionId, shotPath, consoleLines}) {
  const assert = (v, message) => { if (!v) throw new Error(message); };
  const state = () => evaluate("({...document.getElementById('wake-terminal').dataset})");
  const records = () => evaluate(`new Promise((resolve,reject)=>{const r=indexedDB.open('lucy-room-memory-v1');r.onsuccess=()=>{const db=r.result;const q=db.transaction('messages').objectStore('messages').getAll();q.onsuccess=()=>{resolve(q.result);db.close();};q.onerror=()=>reject(q.error);};r.onerror=()=>reject(r.error);})`);
  const panel = (marker='Lucy, wake') => evaluate(`(()=>{for(const [canvas,rows] of window.__roomPanelText){if(!rows.some(r=>r.text===${JSON.stringify(marker)}))continue;const scale=rows[0].scale;return {w:canvas.width/scale,h:canvas.height/scale,rows};}return null;})()`);
  const click = async (label, marker='Lucy, wake') => {
    const p = await panel(marker), row = p.rows.find(r=>r.text===label);
    assert(row,'missing control '+label);
    const size = await evaluate('({w:innerWidth,h:innerHeight})');
    const top = marker==='Lucy, wake' ? (size.h-p.h)/2 : size.h-16-p.h;
    for(const type of ['mousePressed','mouseReleased']) await send('Input.dispatchMouseEvent',{type,x:(size.w-p.w)/2+row.x+8,y:top+row.y+6,button:'left',clickCount:1},sessionId);
    await sleep(150);
  };
  const screenshot = async path => {const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);writeFileSync(path,Buffer.from(shot.data,'base64'));};
  const passive = p => {
    const forbidden=['Start','Play','Pause','Next','Skip','Replay','Open Talk','Load my words · 651 MB','Continue · local model','Send ↵','Stop','Retry save','↑ Older','↓ Newer','End','⌨'];
    assert(!p.rows.some(r=>forbidden.includes(r.text)),'playback or conversation action present');
    assert(p.rows.some(r=>r.text==='× Close'),'labeled close button missing');
    assert(p.rows.some(r=>r.text==='Press Esc to close'),'Escape hint missing');
    assert(p.rows.every(r=>r.x>=0&&r.y>=0&&r.x+r.width<=p.w+1&&r.y+12<=p.h+1),'simulation text clipped');
  };
  await waitFor('private journal',"document.getElementById('journal-status').textContent.includes('private')");
  const saved=await records();
  // An existing Talk draft must remain untouched by watching the simulation.
  await evaluate(`(()=>{const i=document.getElementById('composer-input');i.focus();i.value='My existing Talk draft';i.dispatchEvent(new Event('input',{bubbles:true}));})()`);await key('Escape');
  if(shotPath) await screenshot(shotPath.replace('.png','-room.png'));
  const start=Date.now();
  for(const type of ['mousePressed','mouseReleased']) await send('Input.dispatchMouseEvent',{type,x:380,y:190,button:'left',clickCount:1},sessionId);
  await waitFor('scene screen starts',"document.getElementById('wake-terminal').dataset.open==='true' && document.getElementById('wake-terminal').dataset.step==='1'");
  const seen=new Set();let phone=false;
  // Observe every automatic step without sending another input to the room.
  while(Date.now()-start<10000) {
    const s=await state();seen.add(Number(s.step));passive(await panel());
    if(!phone && Number(s.step)>=4) {
      await send('Emulation.setDeviceMetricsOverride',{width:320,height:700,deviceScaleFactor:2,mobile:true},sessionId);await sleep(100);phone=true;
    }
    if(s.state==='Simulation complete')break;
    await sleep(100);
  }
  const durationMs=Date.now()-start;
  assert((await state()).state==='Simulation complete','single-click simulation did not finish');
  assert(seen.size===7,'an automatic step was skipped');
  assert((await text('wake-terminal')).includes('Lucy: cached'),'freshness example missing');
  assert((await text('wake-terminal')).includes('https://dhilipsiva.dev/lucy/room/'),'room invitation missing');
  let p=await panel();passive(p);assert(p.w<=320&&p.h<=700,'simulation exceeds phone');
  assert(p.rows.some(r=>r.text==='https://dhilipsiva.dev/lucy/room/'),'room invitation not drawn');
  if(shotPath)await screenshot(shotPath.replace('.png','-phone.png'));
  await click('× Close');await waitFor('phone close button dismisses',"document.getElementById('wake-terminal').dataset.open==='false'");
  await key('t');await waitFor('phone reopens',"document.getElementById('wake-terminal').dataset.open==='true'");
  passive(await panel());
  await key('Escape');await waitFor('phone Escape dismisses',"document.getElementById('wake-terminal').dataset.open==='false'");
  await send('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false},sessionId);await sleep(250);
  await key('t');await waitFor('desktop completes again',"document.getElementById('wake-terminal').dataset.state==='Simulation complete'");
  if(shotPath)await screenshot(shotPath);
  await key('Escape');await waitFor('dismissed',"document.getElementById('wake-terminal').dataset.open==='false'");
  await click('Wake demo','Standing');await waitFor('Controls starts a new observation',"document.getElementById('wake-terminal').dataset.open==='true' && document.getElementById('wake-terminal').dataset.step==='1'");
  await click('× Close');await waitFor('desktop close button dismisses',"document.getElementById('wake-terminal').dataset.open==='false'");
  assert(JSON.stringify(await records())===JSON.stringify(saved),'watching changed the journal');
  assert(await evaluate('window.__wakeWordsWorkers===0 && window.__wakeModelRequests.length===0'),'simulation used a model');
  assert(await evaluate("document.getElementById('composer-input').value==='My existing Talk draft'"),'simulation changed the Talk draft');
  // Capture front and oblique views for reviewing the attached smile.
  if(shotPath) {
    // The idle animation starts swimming after 28 seconds; reset to the entrance pose.
    await send('Page.reload',{},sessionId);
    await waitFor('fresh face view',"document.getElementById('status')?.textContent.startsWith(\"I'm Lucy\")");
    await send('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:2,mobile:false},sessionId);
    await key('h');
    for(let i=0;i<2;i++){await send('Input.dispatchMouseEvent',{type:'mouseWheel',x:640,y:400,deltaX:0,deltaY:-100},sessionId);await sleep(100);}
    await sleep(300);await screenshot(shotPath.replace('.png','-face-front.png'));
    await send('Input.dispatchKeyEvent',{type:'keyDown',key:'ArrowRight',code:'ArrowRight'},sessionId);await sleep(450);
    await send('Input.dispatchKeyEvent',{type:'keyUp',key:'ArrowRight',code:'ArrowRight'},sessionId);await sleep(150);
    await screenshot(shotPath.replace('.png','-face-oblique.png'));
  }
  assert(!consoleLines.some(l=>/panicked|exception:/.test(l)),'browser exception');
  return {pass:true,steps:[...seen],durationMs,modelWorkers:0,modelRequests:0,journalUnchanged:true,checks:['one-click automatic wake','no playback or conversation actions','scene and Controls entry','desktop and phone text containment','public invitation','persistent labeled Close button and Escape hint','Close and Escape dismiss on desktop and phone','dismiss and reopen','Talk draft preserved','no model or journal activity']};
}
