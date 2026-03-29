# Modularity Review

**Scope**: Retro Tetris (Rust/WASM) — post-refactor, full codebase (`tetris-wasm`, `web/`, `tetris-server`)
**Date**: 2026-03-29

## Executive Summary

Retro Tetris is a browser Tetris game built in Rust compiled to WebAssembly, with a Dellacherie AI, served by a self-contained Rust binary. The previous significant issue — JavaScript duplicating the Rust game model — has been fully resolved: all AI evaluation now lives in `lib.rs` and the JS is a thin orchestrator calling a single `best_move()` export. The overall [modularity](https://coupling.dev/posts/core-concepts/modularity/) is healthy. Two minor issues remain, both tolerable at the current scale and [volatility](https://coupling.dev/posts/dimensions-of-coupling/volatility/).

## Coupling Overview

| Integration | [Strength](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) | [Distance](https://coupling.dev/posts/dimensions-of-coupling/distance/) | [Volatility](https://coupling.dev/posts/dimensions-of-coupling/volatility/) | [Balanced?](https://coupling.dev/posts/core-concepts/balance/) |
|---|---|---|---|---|
| `web/index.html` (AI) → `tetris-wasm` | [Contract](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) (`#[wasm_bindgen]` exports) | Very low (same binary, same team) | Low | Yes ✓ |
| `tetris-server` → `web/` assets | [Functional](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) (file paths, MIME types) | Very low (same crate, compile-time) | Low | Yes ✓ |
| `ai_evaluate()` → `Game::clear_lines()` logic | [Functional](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) (duplicated line-clear algorithm) | Zero (same file) | Low | Yes (distance saves it) |

---

## Issue: Line-Clear Logic Exists in Two Places Within `lib.rs`

**Integration**: `ai_evaluate()` → `Game::clear_lines()` (both in `tetris-wasm/src/lib.rs`)
**Severity**: Minor

### Knowledge Leakage

Both functions implement "scan for full rows, remove them, shift the board down":

- `Game::clear_lines()` (lines 297–314) — the authoritative game implementation; also updates score, level, and drop interval.
- `ai_evaluate()` (lines 461–473) — a simulation copy used to count cleared lines and eroded cells for the Dellacherie heuristic.

The shared knowledge is the board mutation rule for a cleared line. The simulation version correctly omits the score/level side effects — but the row-scanning and shifting logic is written twice. This is [functional coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) at zero [distance](https://coupling.dev/posts/dimensions-of-coupling/distance/).

### Complexity Impact

Because this is entirely within `lib.rs`, the distance is zero. A developer modifying `clear_lines` has no signal that `ai_evaluate` contains a structural duplicate. If the line-clearing rule ever changes — adding cascade clears, gravity after clearing, or Tetris-style "flash then remove" timing — both locations must be updated manually. The [coupling](https://coupling.dev/posts/core-concepts/coupling/) is implicit: no function call, no shared type, just parallel implementations of the same rule.

### Cascading Changes

The scenarios that trigger a cascade are narrow but real:

- Changing the board's row representation (e.g., from `Vec<Vec<u8>>` to a bitmask) would require updating both clearing loops independently.
- Adding combo detection or T-spin recognition to `clear_lines` would need a matching update in `ai_evaluate` to keep the AI's predictions accurate.

Because distance is zero and [volatility](https://coupling.dev/posts/dimensions-of-coupling/volatility/) is low, this is tolerable now. It becomes a maintenance hazard if board representation or clearing semantics change.

### Recommended Improvement

Extract a pure board-simulation function that takes a board snapshot and placed piece cells, clears full rows, and returns the result. Both `clear_lines` and `ai_evaluate` call it. Because distance is already zero (same file), this is a refactor with no architectural cost — just a helper function:

```rust
/// Simulate locking piece cells onto board, clear full rows.
/// Returns (new_board, lines_cleared, eroded_piece_cells).
fn simulate_lock(board: &[u8; 200], piece_cells: &[(usize, usize)]) -> ([u8; 200], i32, i32) { ... }
```

`Game::clear_lines` calls this and applies score/level updates on top. `ai_evaluate` calls this to get the post-lock board for heuristic computation. The line-clearing rule lives in exactly one place.

---

## Issue: Orphaned Public API Exports

**Integration**: `web/index.html` → `tetris-wasm` public API surface
**Severity**: Minor

### Knowledge Leakage

After the refactor, two `#[wasm_bindgen]` exports are no longer called by any consumer:

- `get_locked_board() -> Vec<u8>` (line 77)
- `get_piece_type() -> u8` (line 83)

They remain compiled into `web/pkg/tetris_wasm_bg.wasm` and declared in `web/pkg/tetris_wasm.js` and `web/pkg/tetris_wasm.d.ts`. They add surface area to the public [contract](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) that future maintainers must understand and reason about — even though they serve no current consumer.

### Complexity Impact

A developer reading `lib.rs` sees two exports with doc comments explaining their JS-facing purpose. Since `index.html` no longer imports them, the gap between "what the API advertises" and "what is actually used" creates confusion: are these intentionally kept for future use, or are they forgotten leftovers? `#[wasm_bindgen]` prevents the Rust compiler from flagging them as dead code, so the noise is permanent unless explicitly removed.

### Cascading Changes

There are no cascading changes today. The risk is purely cognitive: orphaned exports accumulate into dead-code noise that makes the API harder to understand and maintain over time.

### Recommended Improvement

Remove `get_locked_board` and `get_piece_type` from the `#[wasm_bindgen]` public API. If retained as internal helpers, remove the `#[wasm_bindgen]` attribute and keep them as private Rust functions — the compiler will then enforce their usage. If unused entirely, delete them. Rebuilding `web/pkg/` after removal will shrink the generated JS/WASM slightly and tighten the public contract.

---

## What Is Working Well

- **The significant issue from the previous review is fully resolved.** The JavaScript AI no longer contains any Tetris domain knowledge. The refactored `aiTick()` is 6 lines: poll `get_spawn_count()`, call `best_move()`, forward to `queue_ai_move()`. All piece shapes, rotation logic, placement simulation, and Dellacherie scoring live in Rust with a single source of truth.
- **The `best_move()` → `queue_ai_move()` contract is minimal and explicit.** The boundary between JS and Rust is a two-element `Vec<i32>` with a documented sentinel value (`-1`). No game model knowledge crosses the boundary. This is textbook [contract coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) at near-zero distance.
- **`tetris-server` remains appropriately thin.** No game knowledge; low-strength [functional coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) to `web/` assets; build ordering is now explicit via `Makefile`.
- **All integrations are [balanced](https://coupling.dev/posts/core-concepts/balance/).** No integration is both unbalanced and volatile. The [modularity](https://coupling.dev/posts/core-concepts/modularity/) improvements from the previous review cycle are holding.

---

*This analysis was performed using the [Balanced Coupling](https://coupling.dev) model by [Vlad Khononov](https://vladikk.com).*
