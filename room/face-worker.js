import init, { worker_main } from './face/pkg/lucy_face.js';
init().then(worker_main, (e) => postMessage({ fatal: String(e) }));
