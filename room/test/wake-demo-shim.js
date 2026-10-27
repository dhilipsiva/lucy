// Explicit browser test fixture; production uses the existing real Words worker.
const OriginalWorker = window.Worker;
window.__wakeWorkers = 0;
window.__wakeRequests = [];
window.__wakeCase = 'complete';
window.Worker = class {
  constructor(url, options) {
    if (!String(url).includes('role=words')) return new OriginalWorker(url, options);
    window.__wakeWorkers++;
    this.dead = false;
    setTimeout(() => this.reply({kind:'boot'}), 20);
  }
  reply(message) { if (!this.dead) this.onmessage?.({data:JSON.stringify(message)}); }
  postMessage(raw) {
    const m = JSON.parse(raw);
    window.__wakeRequests.push(m);
    if (m.kind === 'load') {
      if (window.__wakeCase !== 'loading') setTimeout(() => this.reply({kind:'ready'}), 30);
    } else if (m.kind === 'generate') {
      this.id = m.id;
      if (window.__wakeCase === 'fault') {
        setTimeout(() => !this.dead && this.onerror?.({message:'Demo fixture worker failure'}), 30);
        return;
      }
      this.text = `Visible local reply: ${m.question}`;
      setTimeout(() => this.reply({kind:'delta', id:m.id, text:this.text, generated:12}), 40);
      if (window.__wakeCase !== 'stream') setTimeout(() => this.reply({kind:'done', id:m.id, text:this.text, interrupted:false}), 350);
    } else if (m.kind === 'stop') {
      this.reply({kind:'done', id:this.id, text:this.text, interrupted:true});
    }
  }
  terminate() { this.dead = true; }
};
