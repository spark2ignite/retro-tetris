# Modularity Review

**Scope**: Retro Tetris (Rust/WASM) — full codebase (`tetris-wasm`, `web/`, `tetris-server`)
**Date**: 2026-03-29

## Executive Summary

Retro Tetris is a browser-based Tetris game built in Rust compiled to WebAssembly, served by a self-contained Rust binary. The project has three components with clean boundaries: game logic in `tetris-wasm`, a JavaScript frontend with an embedded Dellacherie AI in `web/`, and a minimal HTTP server in `tetris-server`. The overall [modularity](https://coupling.dev/posts/core-concepts/modularity/) is healthy — one significant issue and one minor operational concern were found. No urgent restructuring is needed.

The most important finding is that the JavaScript AI re-implements Tetris domain knowledge already defined in Rust (piece shapes, rotation, placement), creating [implicit model coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) that will silently break if the game rules are ever extended.

## Coupling Overview

| Integration | [Strength](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) | [Distance](https://coupling.dev/posts/dimensions-of-coupling/distance/) | [Volatility](https://coupling.dev/posts/dimensions-of-coupling/volatility/) | [Balanced?](https://coupling.dev/posts/core-concepts/balance/) |
|---|---|---|---|---|
| `web/index.html` (AI) → `tetris-wasm` | [Functional](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) + implicit [Model](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) | Low (same binary, same team) | Low (stable game rules) | No — strength exceeds distance |
| `tetris-server` → `web/` assets | [Functional](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) (file paths, MIME types) | Very low (same crate, compile-time) | Low (wasm-pack output format rarely changes) | Yes |
| `web/index.html` → `web/pkg/` (WASM loader) | [Contract](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) (`#[wasm_bindgen]` exports) | Very low (same binary) | Low | Yes |

---

## Issue: JavaScript AI Duplicates the Rust Game Model

**Integration**: `web/index.html` → `tetris-wasm/src/lib.rs`
**Severity**: Significant

### Knowledge Leakage

The JavaScript Dellacherie AI re-implements core Tetris domain knowledge that already lives authoritatively in Rust. This is [implicit model coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) — the shared knowledge is the game's rules and data representation, but it exists in two places with no explicit contract between them:

| Knowledge | Rust (`lib.rs`) | JS (`index.html`) |
|---|---|---|
| Board dimensions | `const COLS: usize = 10; const ROWS: usize = 20;` | `const ROWS = 20, COLS = 10;` (line 308) |
| Piece shapes | `SHAPES` array (lines 23–32) | `BASE` array (lines 331–340) |
| Rotation logic | `rotate_matrix()` (lines 122–133) | `rotateCW()` (lines 310–316) |
| Placement check | `fits()` (lines 140–152) | `canPlace()` (lines 362–370) |
| Drop simulation | `ghost_row()` (lines 154–157) | `dropY()` (lines 373–377) |

The AI does use proper exported functions (`queue_ai_move`, `get_locked_board`, `get_piece_type`, `get_spawn_count`) for its integration contract — but the evaluation logic duplicates the model rather than delegating to it.

### Complexity Impact

A developer modifying the game model in `lib.rs` — adding wall kicks, changing board dimensions, introducing a new rotation system — has no signal that corresponding changes are required in the JavaScript AI. There is no compile-time error, no runtime error, and no test that would catch the drift. The AI will continue to run and appear to work while computing placements based on outdated rules. This is a hidden [coupling](https://coupling.dev/posts/core-concepts/coupling/) that exceeds the cognitive capacity of a single change: modifying one function in Rust silently invalidates five functions in JavaScript.

### Cascading Changes

Any of the following changes in `tetris-wasm` would require manual, undiscoverable updates in `web/index.html`:

- Changing `COLS` or `ROWS` — the AI's board indexing breaks silently.
- Modifying piece shapes or adding a new piece type — the AI evaluates against the old shapes.
- Changing rotation semantics (e.g. implementing SRS wall kicks) — AI rotations diverge from actual game rotations.
- Changing the board encoding in `get_locked_board()` — the AI's `canPlace()` would misread the board.

Because the [distance](https://coupling.dev/posts/dimensions-of-coupling/distance/) is low (same repo, same team), the cascading change itself is cheap — but it is *invisible*, which makes it dangerous.

### Recommended Improvement

Move the Dellacherie evaluation into `tetris-wasm` and export only the result. Add a single `#[wasm_bindgen]` function:

```rust
/// Returns (rotations, target_x) for the best move given the current state.
#[wasm_bindgen]
pub fn best_move() -> Vec<i32> { ... }
```

The JavaScript AI becomes a thin orchestrator — it polls `get_spawn_count()`, calls `best_move()`, and passes the result to `queue_ai_move()`. All piece shapes, rotation logic, and placement simulation remain in Rust with a single source of truth.

**Trade-off:** This adds ~100 lines to `lib.rs` and requires the AI toggle logic to stay in JS (which is fine — it's UI behaviour, not game logic). The benefit is that any future change to the game model is automatically reflected in the AI, and the integration is fully expressed through the existing `#[wasm_bindgen]` [contract coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) pattern already in use.

---

## Issue: Build Pipeline Ordering Has No Enforcement

**Integration**: `tetris-server` → `web/pkg/` (compiled WASM artifacts)
**Severity**: Minor

### Knowledge Leakage

`tetris-server` embeds compiled WASM artifacts at build time via `include_bytes!`:

```rust
const HTML: &[u8] = include_bytes!("../web/index.html");
const JS:   &[u8] = include_bytes!("../web/pkg/tetris_wasm.js");
const WASM: &[u8] = include_bytes!("../web/pkg/tetris_wasm_bg.wasm");
```

The server knows the specific file paths and format of `wasm-pack` output. There is a mandatory build order — `wasm-pack build` must precede `cargo build --release` — but this ordering is undocumented in the tooling and enforced only by README instructions.

### Complexity Impact

A developer who modifies `tetris-wasm/src/lib.rs` and runs `cargo build` without first running `wasm-pack build` will produce a server binary containing stale game logic. Cargo will not warn them. The binary will compile and run. The mismatch between `lib.rs` and `web/pkg/` is invisible until the game behaves incorrectly at runtime.

### Cascading Changes

- Modifying any exported `#[wasm_bindgen]` function in `lib.rs` → must rebuild `web/pkg/` → must rebuild `tetris-server`. Currently: two manual steps with no dependency tracking.
- Adding a new WASM export used in `index.html` → same three-step chain, all manual.

Because [volatility](https://coupling.dev/posts/dimensions-of-coupling/volatility/) is low (the build artifact format rarely changes) and distance is very low, this is tolerable. But it will silently mislead contributors.

### Recommended Improvement

Add a `Makefile` at the repo root:

```makefile
.PHONY: wasm server run

wasm:
	wasm-pack build tetris-wasm --target web --out-dir ../web/pkg

server: wasm
	cargo build --release -p tetris-server

run: server
	./target/release/tetris-server
```

Alternatively, a `build.rs` in `tetris-server` can emit a `cargo:rerun-if-changed=../tetris-wasm/src/lib.rs` directive and check that `web/pkg/tetris_wasm_bg.wasm` exists and is not older than `lib.rs`, printing a warning if the artifacts are stale. The Makefile is simpler and more visible to contributors.

---

## What Is Working Well

- **`tetris-server` is appropriately thin.** It has no game knowledge — just file paths and MIME types. Its [functional coupling](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/) to `web/` matches its very low [distance](https://coupling.dev/posts/dimensions-of-coupling/distance/).
- **The WASM/JS integration contract is well-designed.** The `SNAP_*` thread-locals and `#[wasm_bindgen]` exports (`queue_ai_move`, `get_locked_board`, etc.) form an explicit, stable [contract](https://coupling.dev/posts/dimensions-of-coupling/integration-strength/). The AI is correctly a consumer of this contract.
- **Module boundaries map cleanly to build units.** Game logic, presentation, and serving are in separate crates/files with no circular dependencies. The [balance](https://coupling.dev/posts/core-concepts/balance/) between strength, distance, and volatility is correct for two out of three integrations.

---

*This analysis was performed using the [Balanced Coupling](https://coupling.dev) model by [Vlad Khononov](https://vladikk.com).*
