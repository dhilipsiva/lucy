import init, { worker_main } from './pkg/lucy_room.js';
init().then(worker_main, (e) => postMessage(JSON.stringify({ kind: 'fault', error: String(e) })));
