/* tslint:disable */
/* eslint-disable */

/**
 * Returns [rot_index, target_x] for the best move, or [-1, -1] if none found.
 */
export function best_move(): Int32Array;

/**
 * Whether the game is in Playing state
 */
export function get_game_active(): boolean;

/**
 * JS reads next piece type (1-7)
 */
export function get_next_piece_type(): number;

/**
 * Increments each spawn — JS polls this to know when a new piece appeared
 */
export function get_spawn_count(): number;

/**
 * JS → Rust: queue an AI move (rotation count + target x after rotation)
 */
export function queue_ai_move(rotations: number, target_x: number): void;

export function run(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly best_move: () => [number, number];
    readonly get_game_active: () => number;
    readonly get_next_piece_type: () => number;
    readonly get_spawn_count: () => number;
    readonly queue_ai_move: (a: number, b: number) => void;
    readonly run: () => void;
    readonly wasm_bindgen__convert__closures_____invoke__h2d1bc05f03f28453: (a: number, b: number, c: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hf5e46da12abc8af2: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
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
