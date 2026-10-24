/* tslint:disable */
/* eslint-disable */

/**
 * One in-memory knowledge base. The page creates one per loaded example;
 * "Reset" just builds a fresh Session and re-asserts the KB lines.
 */
export class Session {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Parse one KR assertion, compile to FOL, assert. A bare-`.i`
     * multi-sentence text becomes N INDEPENDENT facts (one per root; connectives
     * stay whole); returns the minted fact ids in root order. Exact-count and
     * executable compute formulas in asserted position (outside opaque quoted
     * content) are query-only and fail before id allocation.
     */
    assert_text(text: string): BigUint64Array;
    /**
     * Certify a KR query: verdict + proof trace + session profile + the
     * lockstep engine version, bound into one JSON `ProofEnvelope`
     * (`nibli_protocol::envelope_to_json`; `validate_envelope` is the
     * KB-independent checker). The durable pairing a bare trace cannot prove.
     */
    certify(text: string): string;
    /**
     * Whether the legacy existential-import profile is active (default false).
     */
    existential_import_enabled(): boolean;
    /**
     * Retained assertion metadata. Decimal string IDs preserve all 64 bits in JSON.
     */
    list_assertion_records(): string;
    /**
     * All active facts as JSON: `[{ id, label }]`.
     */
    list_facts(): string;
    max_chain_depth(): number;
    constructor();
    /**
     * Enumerate query witnesses as JSON, including each binding's semantic origin.
     */
    query_find(text: string): string;
    /**
     * Run a nibli KR query. Returns JSON:
     * `{ status, detail, existential_import, naf_dependent, cwa_false,
     *    proof_text, why, proof }`.
     */
    query_with_proof(text: string): string;
    /**
     * Clear all facts and rules.
     */
    reset(): void;
    /**
     * Retract a fact by id and rebuild derived state.
     */
    retract_fact(id: bigint): void;
    /**
     * Enable/disable the explicit legacy existential-import profile. The
     * active KB is rebuilt atomically, so the new profile applies immediately.
     */
    set_existential_import(enabled: boolean): void;
    /**
     * DEPRECATED NO-OP compatibility shim: the Lojban front-end retired at
     * THE DROP, so there is no language to select — every session is KR.
     * Kept (accepting and ignoring any string) so deployed-site JS written
     * against the dual-front-end API keeps loading; dies at the "nibli KR"
     * rename milestone.
     */
    set_language(_lang: string): void;
    /**
     * Validate the JavaScript number before any integer coercion.
     */
    set_max_chain_depth(depth: number): void;
}

/**
 * DEPRECATED compatibility shim — now a pure ECHO no-op: the word-by-word
 * Lojban lexical gloss it once produced died with the cmavo layer at the
 * committed-corpus milestone. Kept only so deployed-site JS written against
 * the dual-front-end API keeps loading; the site-migration bullet owns the
 * deletion of this and `set_language` together.
 */
export function back_translate(word: string): string;

/**
 * IR-driven back-translation: parse + compile to FOL, then render structure-
 * exposing English via nibli-render. This is the default Transparency Triad
 * reading; it falls back to the lexical gloss when the input does not compile.
 */
export function back_translate_ir(text: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_session_free: (a: number, b: number) => void;
    readonly back_translate: (a: number, b: number, c: number) => void;
    readonly back_translate_ir: (a: number, b: number, c: number) => void;
    readonly session_assert_text: (a: number, b: number, c: number, d: number) => void;
    readonly session_certify: (a: number, b: number, c: number, d: number) => void;
    readonly session_existential_import_enabled: (a: number) => number;
    readonly session_list_assertion_records: (a: number, b: number) => void;
    readonly session_list_facts: (a: number, b: number) => void;
    readonly session_max_chain_depth: (a: number) => number;
    readonly session_new: () => number;
    readonly session_query_find: (a: number, b: number, c: number, d: number) => void;
    readonly session_query_with_proof: (a: number, b: number, c: number, d: number) => void;
    readonly session_reset: (a: number, b: number) => void;
    readonly session_retract_fact: (a: number, b: number, c: bigint) => void;
    readonly session_set_existential_import: (a: number, b: number, c: number) => void;
    readonly session_set_language: (a: number, b: number, c: number, d: number) => void;
    readonly session_set_max_chain_depth: (a: number, b: number, c: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
