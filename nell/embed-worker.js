// The neural vector space, off the main thread: a small sentence-embedding model run by
// transformers.js (pinned version and hash in vendor/REMOTE; model and revision given by the
// page). Protocol:
//   main → worker {cmd:'load', lib:{url, sha256}, model:{id, revision, dtype}}
//   worker → main {status:'progress', frac, label} | {status:'ready', dim} | {status:'error', error}
//   main → worker {cmd:'embed', id, texts}
//   worker → main {status:'done', id, dim, data: Float32Array (n × dim)} | {status:'error', id, error}
import { verifiedImport } from './verify.js';

let extractor = null;

self.onmessage = async (e) => {
  const msg = e.data;
  if (msg.cmd === 'load') {
    try {
      self.postMessage({ status: 'progress', frac: 0, label: 'fetching the embedding library (pinned, hashed)…' });
      const tf = await verifiedImport(msg.lib);
      tf.env.allowLocalModels = false;
      tf.env.useBrowserCache = true;
      const wasm = tf.env.backends?.onnx?.wasm;
      if (wasm) { wasm.numThreads = 1; wasm.proxy = false; }
      extractor = await tf.pipeline('feature-extraction', msg.model.id, {
        revision: msg.model.revision,
        dtype: msg.model.dtype || 'q8',
        device: 'wasm',
        progress_callback: (p) => {
          if (p.status === 'progress' && p.file) self.postMessage({ status: 'progress', frac: (p.progress || 0) / 100, label: `${p.file} ${Math.round(p.progress || 0)}%` });
          else if (p.status === 'ready') self.postMessage({ status: 'progress', frac: 1, label: 'model ready' });
        },
      });
      const probe = await extractor(['Nell'], { pooling: 'mean', normalize: true });
      self.postMessage({ status: 'ready', dim: probe.dims[1] });
    } catch (err) {
      extractor = null;
      self.postMessage({ status: 'error', error: String((err && err.message) || err) });
    }
    return;
  }
  if (msg.cmd === 'embed') {
    if (!extractor) { self.postMessage({ status: 'error', id: msg.id, error: 'model not loaded' }); return; }
    try {
      const out = await extractor(msg.texts, { pooling: 'mean', normalize: true });
      const data = out.data instanceof Float32Array ? Float32Array.from(out.data) : Float32Array.from(out.data);
      self.postMessage({ status: 'done', id: msg.id, dim: out.dims[1], data }, [data.buffer]);
    } catch (err) {
      self.postMessage({ status: 'error', id: msg.id, error: String((err && err.message) || err) });
    }
  }
};
