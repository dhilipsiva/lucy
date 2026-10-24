// My words in the room (roadmap/N2b.md): the same fine-tuned weights and revision that
// dhilipsiva.dev/play loads, Qwen3-1.7B on WebGPU through WebLLM or Qwen3-0.6B on the CPU
// through candle, each in a module worker. The prompt is the model's own training system
// prompt, then what the room retrieved on this device (the engine's verdicts first, then
// records and facts), then the question. The words are words; nothing here is a verdict.

export const LUCY_REV = 'a082662b4403665ab1910795fae279481b9f4d3a'; // dhilipsiva/lucy-slm
export const LUCY_BASE = `https://huggingface.co/dhilipsiva/lucy-slm/resolve/${LUCY_REV}`;
// SRI hashes of that revision's WebGPU files, as the site holds them: a changed file fails to load instead of running.
export const LUCY_INTEGRITY = {
  q4f16: {
    config: 'sha256-zUsHETLP5I4p7yQ4lTlKI+Zfr2fl821E/m4BELpoPw8=',
    model_lib: 'sha256-gWGqpLQLzPGfztsvLowiHrnvty0hmGgfGVjJweBaaC8=',
    tokenizer: { 'tokenizer.json': 'sha256-vnVgYJPbIJTXzSDzwvOFwhJ1Bki9bqT7K/UHpqTFVQY=' },
  },
  q4f32: {
    config: 'sha256-XZtdmFPF/qDwgqF2WhMh3g9HjztXVozGTsfKejAvrJ8=',
    model_lib: 'sha256-qAyg0kXtnOSSSXkYr9IewdQ904c7Y1kSVbAb2cQa32U=',
    tokenizer: { 'tokenizer.json': 'sha256-vnVgYJPbIJTXzSDzwvOFwhJ1Bki9bqT7K/UHpqTFVQY=' },
  },
};
// The runtime, pinned and hashed (vendor/REMOTE); verify.js checks each before use.
export const RUNTIME = {
  webllm: { url: 'https://dhilipsiva.dev/play/vendor/web-llm-0.2.85/index.js', sha256: '341bae95822bfee1d0fd6a0e6cd2db8613bb8edf809390ac142fba36ec17792c' },
  slmJs: { url: 'https://dhilipsiva.dev/play/wasm/slm_wasm.js', sha256: '93193eb5959da6b15af81cfa9a4be6cf8c83fa2bb48ab2f95a155c638aaf1552' },
  slmWasm: { url: 'https://dhilipsiva.dev/play/wasm/slm_wasm_bg.wasm', sha256: '99f3fe4ee9a14de607f723771356b94282f61fe1af4640c4d038eb31bf08969e' },
};
export const SPEC = {
  model: `${LUCY_BASE}/lucy-0.6b-q8_0.gguf`,
  tokenizer: `${LUCY_BASE}/tokenizer.json`,
  systemUrl: `${LUCY_BASE}/system.txt`,
  webgpu: {
    q4f16: { model: `${LUCY_BASE}/mlc/lucy-1.7b-q4f16_1/`, lib: `${LUCY_BASE}/lib/Qwen3-1.7B-q4f16_1_cs1k-webgpu.wasm`, integrity: LUCY_INTEGRITY.q4f16 },
    q4f32: { model: `${LUCY_BASE}/mlc/lucy-1.7b-q4f32_1/`, lib: `${LUCY_BASE}/lib/Qwen3-1.7B-q4f32_1_cs1k-webgpu.wasm`, integrity: LUCY_INTEGRITY.q4f32 },
  },
  // Qwen3 opens a <think> block unless the prompt closes an empty one; the fine-tune trained with exactly this prefix.
  assistantPrefix: '<think>\n\n</think>\n\n',
  sampling: { temp: 0.3, topP: 0.9, repeatPenalty: 1.05 },
};
// The CPU model's context is 1024 tokens and the WebGPU one's 2048, so the prompt is clamped hard.
export const LIMITS = { questionChars: 400, contextChars: 1500, lineChars: 220, historyChars: 600, maxTokens: 160 };

/** Retrieved text never carries chat control tokens into the prompt. */
export function sanitize(text) {
  return String(text ?? '').replace(/<\|im_(start|end)\|>/g, ' ').replace(/<\/?think>/g, ' ').replace(/\s+/g, ' ').trim();
}
const clip = (s, n) => (s.length > n ? s.slice(0, n - 1) + '…' : s);

/**
 * The lines fed to the model: verdict lines first and whole (they are the engine's), then
 * records, then facts, each clipped, the whole within the budget.
 * records: [{when, speaker, peer, kind, text}]; facts: [{english, kr}]; verdicts: strings.
 */
export function buildContext({ verdicts = [], records = [], facts = [], limits = LIMITS }) {
  const lines = [];
  let used = 0;
  const push = (line) => {
    if (used + line.length + 1 > limits.contextChars) return false;
    lines.push(line); used += line.length + 1; return true;
  };
  for (const v of verdicts) push(`[verdict] ${sanitize(v)}`);
  for (const r of records) {
    const head = `[record ${r.when || '?'} · ${sanitize(r.speaker || '?')} · ${sanitize(r.peer || '?')}${r.kind && r.kind !== 'message' ? ` · ${sanitize(r.kind)}` : ''}] `;
    if (!push(head + clip(sanitize(r.text), limits.lineChars))) break;
  }
  for (const f of facts) if (!push(clip(`[fact] ${sanitize(f.english || f.kr)} (${sanitize(f.kr)})`, limits.lineChars))) break;
  return lines.join('\n');
}

function clampHistory(history, budget) {
  const kept = []; let used = 0;
  for (let i = history.length - 1; i >= 0; i--) {
    const m = history[i];
    if (!m || (m.role !== 'user' && m.role !== 'assistant') || !m.content) continue;
    const c = clip(sanitize(m.content), 200);
    if (used + c.length > budget) break;
    used += c.length; kept.unshift({ role: m.role, content: c });
  }
  return kept;
}

/** The raw ChatML prompt, byte-compatible with the fine-tune's training format. */
export function buildPrompt({ system, context, question, history = [], assistantPrefix = SPEC.assistantPrefix, limits = LIMITS }) {
  const q = clip(sanitize(question), limits.questionChars);
  const user = (context
    ? 'What my memory holds, retrieved on this device; the verdicts are the engine\'s and not mine to change:\n' + context +
      '\n\nAnswer from these lines where they bear on the question; where they do not, say that my memory does not hold it.\n\n'
    : '') + `Question: ${q}`;
  const hist = clampHistory(history, limits.historyChars);
  return '<|im_start|>system\n' + system + '<|im_end|>\n' +
    hist.map((m) => `<|im_start|>${m.role}\n${m.content}<|im_end|>\n`).join('') +
    '<|im_start|>user\n' + user + '<|im_end|>\n' +
    '<|im_start|>assistant\n' + assistantPrefix;
}

/** The model in a worker: load once on request, then say() per question, streaming pieces. */
export class Brain {
  constructor({ log = () => {} } = {}) {
    this.log = log; this.worker = null; this.engine = null; this.variant = null; this.system = null;
    this.loading = false; this.pending = new Map(); this.seq = 0;
  }
  get ready() { return !!this.worker; }
  static async hasWebGPU() {
    try { return !!(navigator.gpu && await navigator.gpu.requestAdapter()); } catch { return false; }
  }
  describe() {
    return this.engine === 'WebGPU'
      ? `Qwen3-1.7B fine-tune on WebGPU (WebLLM${this.variant ? `, ${this.variant}` : ''})`
      : 'Qwen3-0.6B fine-tune on the CPU (candle)';
  }
  async load(onProgress = () => {}) {
    if (this.worker) return true;
    if (this.loading) return false;
    this.loading = true;
    try {
      if (!this.system) {
        const res = await fetch(SPEC.systemUrl);
        if (!res.ok) throw new Error(`system.txt ${res.status}`);
        this.system = (await res.text()).replace(/\n+$/, '');
      }
      const gpu = await Brain.hasWebGPU();
      const w = new Worker(new URL(gpu ? './brain-webgpu-worker.js' : './brain-cpu-worker.js', import.meta.url), { type: 'module' });
      const ok = await new Promise((resolve) => {
        w.onerror = (err) => { this.log(`words: worker error: ${err.message || err}`); resolve(false); };
        w.onmessage = (e) => {
          const m = e.data;
          if (m.status === 'progress') onProgress(m.frac, m.label);
          else if (m.status === 'ready') { this.variant = m.variant || null; resolve(true); }
          else if (m.status === 'error') { this.log(`words: ${m.error}`); resolve(false); }
        };
        onProgress(0, gpu ? 'starting the WebGPU engine…' : 'starting the CPU engine…');
        w.postMessage(gpu
          ? { cmd: 'load', runtime: RUNTIME, webgpu: SPEC.webgpu, keep: LUCY_BASE }
          : { cmd: 'load', runtime: RUNTIME, ggufUrl: SPEC.model, tokUrl: SPEC.tokenizer });
      });
      if (!ok) { try { w.terminate(); } catch { /* gone */ } return false; }
      this.worker = w; this.engine = gpu ? 'WebGPU' : 'CPU';
      w.onmessage = (e) => this.onMessage(e.data);
      w.onerror = (err) => this.log(`words: worker error: ${err.message || err}`);
      return true;
    } catch (e) {
      this.log(`words: ${e.message || e}`);
      return false;
    } finally { this.loading = false; }
  }
  onMessage(m) {
    const p = this.pending.get(m.id);
    if (!p) return;
    if (m.status === 'token') p.onToken?.(m.piece);
    else if (m.status === 'done') { clearTimeout(p.timer); this.pending.delete(m.id); p.resolve(m.text); }
    else if (m.status === 'error') { clearTimeout(p.timer); this.pending.delete(m.id); p.reject(new Error(m.error)); }
  }
  async say({ question, context, history = [], onToken, timeoutMs = 120000 }) {
    if (!this.worker) throw new Error('the model is not loaded');
    const prompt = buildPrompt({ system: this.system, context, question, history });
    const id = ++this.seq;
    const raw = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('inference timeout')); }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, onToken });
      this.worker.postMessage({ cmd: 'generate', id, prompt, ...SPEC.sampling, maxTokens: LIMITS.maxTokens, seed: Date.now() >>> 0 });
    });
    const text = raw.split('<|im_end|>')[0].split('<|im_start|>')[0].replace(/<think>[\s\S]*?<\/think>/g, '').replace(/<\/?think>/g, '').trim();
    if (!text) throw new Error('empty completion');
    return { text, engine: this.describe(), revision: LUCY_REV, promptChars: prompt.length };
  }
}
