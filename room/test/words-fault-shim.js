// Fault and crash-recovery fixture. Real weights are exercised separately by --words.
const NativeWorker=window.Worker;
window.__modelCase='stream';
window.Worker=class {
 constructor(url,opts){if(!String(url).includes('role=words'))return new NativeWorker(url,opts);this.dead=false;setTimeout(()=>this.reply({kind:'boot'}),30);}
 reply(data){if(!this.dead)this.onmessage?.({data:JSON.stringify(data)});}
 postMessage(raw){const m=JSON.parse(raw);if(m.kind==='load')return setTimeout(()=>this.reply({kind:'ready'}),20);if(m.kind==='generate'){this.id=m.id;if(window.__modelCase==='fault')return setTimeout(()=>this.onerror?.({message:'Injected model worker failure'}),20);setTimeout(()=>this.reply({kind:'delta',id:m.id,text:'Visible reply before reload. 🐦',generated:8}),30);}if(m.kind==='stop')this.reply({kind:'done',id:this.id,text:'Visible reply before reload. 🐦',interrupted:true});}
 terminate(){this.dead=true;}
};
