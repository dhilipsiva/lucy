// Observe accidental model use; the wake simulation must never create or load one.
const NativeWorker = window.Worker;
window.__wakeWordsWorkers = 0;
window.__wakeModelRequests = [];
window.Worker = class extends NativeWorker {
  constructor(url, options) {
    if (String(url).includes('role=words')) window.__wakeWordsWorkers++;
    super(url, options);
  }
};
const nativeFetch = window.fetch;
window.fetch = function(input, ...args) {
  const url = String(input?.url || input);
  if (/huggingface\.co|\.gguf(?:$|\?)/.test(url)) window.__wakeModelRequests.push(url);
  return nativeFetch.call(this, input, ...args);
};
