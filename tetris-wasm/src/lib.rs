use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, KeyboardEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// ── Constants ────────────────────────────────────────────────────────────────
const COLS: usize = 10;
const ROWS: usize = 20;
const BS: f64 = 30.0;

const COLORS: [&str; 8] = [
    "",
    "#00e5ff", // I cyan
    "#ffe600", // O yellow
    "#cc00ff", // T purple
    "#00ff44", // S green
    "#ff2222", // Z red
    "#3355ff", // J blue
    "#ff8800", // L orange
];

const SHAPES: [&[&[u8]]; 8] = [
    &[],
    &[&[0,0,0,0], &[1,1,1,1], &[0,0,0,0], &[0,0,0,0]], // I
    &[&[2,2], &[2,2]],                                    // O
    &[&[0,3,0], &[3,3,3], &[0,0,0]],                     // T
    &[&[0,4,4], &[4,4,0], &[0,0,0]],                     // S
    &[&[5,5,0], &[0,5,5], &[0,0,0]],                     // Z
    &[&[6,0,0], &[6,6,6], &[0,0,0]],                     // J
    &[&[0,0,7], &[7,7,7], &[0,0,0]],                     // L
];

const KICKS: [i32; 5] = [0, -1, 1, -2, 2];
const SCORE_TABLE: [u32; 5] = [0, 100, 300, 500, 800];

// ── Thread-local AI state (exported to JS) ───────────────────────────────────
thread_local! {
    // Flat locked board: 200 bytes, row-major
    static SNAP_BOARD:       RefCell<[u8; 200]> = RefCell::new([0u8; 200]);
    // Current piece type 1-7
    static SNAP_PIECE_TYPE:  Cell<u8>  = Cell::new(0);
    // Next piece type 1-7
    static SNAP_NEXT_TYPE:   Cell<u8>  = Cell::new(0);
    // Increments every time a new piece spawns — AI uses this to trigger
    static SNAP_SPAWN_COUNT: Cell<u32> = Cell::new(0);
    // Whether game is actively playing
    static SNAP_ACTIVE:      Cell<bool> = Cell::new(false);
    // AI move request: JS sets this, Rust game loop consumes it
    static AI_ROT:  Cell<i32> = Cell::new(-1); // -1 = no request
    static AI_XPOS: Cell<i32> = Cell::new(0);
}

fn update_snapshots(game: &Game) {
    SNAP_ACTIVE.with(|a| a.set(game.state == State::Playing));
    SNAP_PIECE_TYPE.with(|t| t.set(game.piece.piece_type as u8));
    SNAP_NEXT_TYPE.with(|t| t.set(game.next_piece.piece_type as u8));
    SNAP_BOARD.with(|b| {
        let mut board = b.borrow_mut();
        for r in 0..ROWS {
            for c in 0..COLS {
                board[r * COLS + c] = if game.board[r][c] != 0 { 1 } else { 0 };
            }
        }
    });
}

/// JS → Rust: queue an AI move (rotation count + target x after rotation)
#[wasm_bindgen]
pub fn queue_ai_move(rotations: i32, target_x: i32) {
    AI_ROT.with(|r| r.set(rotations));
    AI_XPOS.with(|x| x.set(target_x));
}

/// JS reads the locked board (200 bytes, 0=empty 1=filled)
#[wasm_bindgen]
pub fn get_locked_board() -> Vec<u8> {
    SNAP_BOARD.with(|b| b.borrow().to_vec())
}

/// JS reads current piece type (1-7)
#[wasm_bindgen]
pub fn get_piece_type() -> u8 {
    SNAP_PIECE_TYPE.with(|t| t.get())
}

/// JS reads next piece type (1-7)
#[wasm_bindgen]
pub fn get_next_piece_type() -> u8 {
    SNAP_NEXT_TYPE.with(|t| t.get())
}

/// Increments each spawn — JS polls this to know when a new piece appeared
#[wasm_bindgen]
pub fn get_spawn_count() -> u32 {
    SNAP_SPAWN_COUNT.with(|c| c.get())
}

/// Whether the game is in Playing state
#[wasm_bindgen]
pub fn get_game_active() -> bool {
    SNAP_ACTIVE.with(|a| a.get())
}

// ── Piece ────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct Piece {
    piece_type: usize,
    matrix: Vec<Vec<u8>>,
    x: i32,
    y: i32,
}

impl Piece {
    fn new(piece_type: usize) -> Self {
        let matrix: Vec<Vec<u8>> = SHAPES[piece_type].iter().map(|row| row.to_vec()).collect();
        let x = (COLS as i32 / 2) - (matrix[0].len() as i32 / 2);
        Piece { piece_type, matrix, x, y: 0 }
    }
}

fn rotate_matrix(m: &Vec<Vec<u8>>, dir: i32) -> Vec<Vec<u8>> {
    let rows = m.len();
    let cols = m[0].len();
    let mut out = vec![vec![0u8; rows]; cols];
    for r in 0..rows {
        for c in 0..cols {
            if dir > 0 { out[c][rows - 1 - r] = m[r][c]; }
            else        { out[cols - 1 - c][r] = m[r][c]; }
        }
    }
    out
}

// ── Board ────────────────────────────────────────────────────────────────────
type Board = Vec<Vec<u8>>;

fn empty_board() -> Board { vec![vec![0u8; COLS]; ROWS] }

fn fits(board: &Board, matrix: &Vec<Vec<u8>>, x: i32, y: i32) -> bool {
    for (r, row) in matrix.iter().enumerate() {
        for (c, &val) in row.iter().enumerate() {
            if val != 0 {
                let nx = x + c as i32;
                let ny = y + r as i32;
                if nx < 0 || nx >= COLS as i32 || ny >= ROWS as i32 { return false; }
                if ny >= 0 && board[ny as usize][nx as usize] != 0 { return false; }
            }
        }
    }
    true
}

fn ghost_row(board: &Board, piece: &Piece) -> i32 {
    let mut gy = piece.y;
    while fits(board, &piece.matrix, piece.x, gy + 1) { gy += 1; }
    gy
}

// ── 7-bag ────────────────────────────────────────────────────────────────────
fn new_bag(rng: &mut u32) -> Vec<usize> {
    let mut bag: Vec<usize> = (1..=7).collect();
    for i in (1..bag.len()).rev() {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 17;
        *rng ^= *rng << 5;
        let j = (*rng as usize) % (i + 1);
        bag.swap(i, j);
    }
    bag
}

// ── Game state ───────────────────────────────────────────────────────────────
#[derive(PartialEq, Clone)]
enum State { Menu, Playing, Paused, Over }

struct Game {
    board: Board,
    piece: Piece,
    next_piece: Piece,
    held_type: Option<usize>,
    can_hold: bool,
    score: u32,
    hi_score: u32,
    level: u32,
    lines_cleared: u32,
    state: State,
    prev_state: State,
    drop_counter: f64,
    drop_interval: f64,
    last_time: f64,
    bag: Vec<usize>,
    rng: u32,
    keys_down: std::collections::HashSet<String>,
    das_key: Option<String>,
    das_timer: f64,
    das_active: bool,
}

impl Game {
    fn new(rng_seed: u32) -> Self {
        let mut rng = rng_seed;
        let mut bag = new_bag(&mut rng);
        let next_type = bag.pop().unwrap();
        let cur_type = { if bag.is_empty() { bag = new_bag(&mut rng); } bag.pop().unwrap() };
        Game {
            board: empty_board(),
            piece: Piece::new(cur_type),
            next_piece: Piece::new(next_type),
            held_type: None, can_hold: true,
            score: 0, hi_score: 0, level: 1, lines_cleared: 0,
            state: State::Menu, prev_state: State::Menu,
            drop_counter: 0.0, drop_interval: 1000.0, last_time: 0.0,
            bag, rng,
            keys_down: std::collections::HashSet::new(),
            das_key: None, das_timer: 0.0, das_active: false,
        }
    }

    fn draw_from_bag(&mut self) -> usize {
        if self.bag.is_empty() { self.bag = new_bag(&mut self.rng); }
        self.bag.pop().unwrap()
    }

    fn advance_piece(&mut self) {
        self.piece = self.next_piece.clone();
        let t = self.draw_from_bag();
        self.next_piece = Piece::new(t);
        self.can_hold = true;
        SNAP_SPAWN_COUNT.with(|c| c.set(c.get() + 1));
    }

    fn start(&mut self) {
        self.board = empty_board();
        self.bag = new_bag(&mut self.rng);
        let t1 = self.draw_from_bag();
        let t2 = self.draw_from_bag();
        self.piece = Piece::new(t1);
        self.next_piece = Piece::new(t2);
        self.held_type = None; self.can_hold = true;
        self.score = 0; self.level = 1; self.lines_cleared = 0;
        self.drop_interval = 1000.0; self.drop_counter = 0.0; self.last_time = 0.0;
        self.state = State::Playing;
        SNAP_SPAWN_COUNT.with(|c| c.set(0));
        AI_ROT.with(|r| r.set(-1));
    }

    fn move_piece(&mut self, dx: i32) {
        if fits(&self.board, &self.piece.matrix, self.piece.x + dx, self.piece.y) {
            self.piece.x += dx;
        }
    }

    fn rotate_piece(&mut self, dir: i32) {
        let nm = rotate_matrix(&self.piece.matrix, dir);
        for &k in &KICKS {
            if fits(&self.board, &nm, self.piece.x + k, self.piece.y) {
                self.piece.matrix = nm; self.piece.x += k; return;
            }
        }
    }

    fn hard_drop(&mut self) {
        let gy = ghost_row(&self.board, &self.piece);
        self.score += 2 * (gy - self.piece.y) as u32;
        self.piece.y = gy;
        self.lock_piece();
    }

    fn hold_piece(&mut self) {
        if !self.can_hold { return; }
        self.can_hold = false;
        match self.held_type {
            None => { self.held_type = Some(self.piece.piece_type); self.advance_piece(); }
            Some(t) => { self.held_type = Some(self.piece.piece_type); self.piece = Piece::new(t); }
        }
    }

    fn lock_piece(&mut self) {
        for (r, row) in self.piece.matrix.iter().enumerate() {
            for (c, &val) in row.iter().enumerate() {
                if val != 0 {
                    let ny = self.piece.y + r as i32;
                    let nx = self.piece.x + c as i32;
                    if ny >= 0 { self.board[ny as usize][nx as usize] = val; }
                }
            }
        }
        self.clear_lines();
        self.advance_piece();
        self.drop_counter = 0.0;
        if !fits(&self.board, &self.piece.matrix, self.piece.x, self.piece.y) {
            self.state = State::Over;
        }
    }

    fn clear_lines(&mut self) {
        let mut n = 0u32;
        let mut r = ROWS as i32 - 1;
        while r >= 0 {
            if self.board[r as usize].iter().all(|&v| v != 0) {
                self.board.remove(r as usize);
                self.board.insert(0, vec![0u8; COLS]);
                n += 1;
            } else { r -= 1; }
        }
        if n > 0 {
            self.score += SCORE_TABLE[n.min(4) as usize] * self.level;
            self.lines_cleared += n;
            self.level = self.lines_cleared / 10 + 1;
            self.drop_interval = (1000.0 - (self.level as f64 - 1.0) * 90.0).max(80.0);
            if self.score > self.hi_score { self.hi_score = self.score; }
        }
    }

    fn update(&mut self, ts: f64) {
        if self.state != State::Playing { return; }
        let dt = if self.last_time > 0.0 { (ts - self.last_time).min(250.0) } else { 0.0 };
        self.last_time = ts;

        // Consume queued AI move
        let rot = AI_ROT.with(|r| r.get());
        if rot >= 0 {
            AI_ROT.with(|r| r.set(-1));
            let target_x = AI_XPOS.with(|x| x.get());
            // Apply rotations
            for _ in 0..rot { self.rotate_piece(1); }
            // Slam to left wall then slide right to target_x
            for _ in 0..COLS { self.move_piece(-1); }
            for _ in 0..target_x as usize { self.move_piece(1); }
            self.hard_drop();
            return;
        }

        // DAS
        if let Some(ref key) = self.das_key.clone() {
            let dx = if key == "ArrowLeft" { -1 } else { 1 };
            self.das_timer += dt;
            if !self.das_active && self.das_timer >= 150.0 { self.das_active = true; self.das_timer = 0.0; }
            if self.das_active && self.das_timer >= 48.0 { self.move_piece(dx); self.das_timer = 0.0; }
        }

        // Gravity
        let soft = self.keys_down.contains("ArrowDown");
        let interval = if soft { self.drop_interval.min(50.0) } else { self.drop_interval };
        self.drop_counter += dt;
        if self.drop_counter >= interval {
            self.drop_counter -= interval;
            if soft {
                if fits(&self.board, &self.piece.matrix, self.piece.x, self.piece.y + 1) {
                    self.piece.y += 1; self.score += 1;
                } else { self.lock_piece(); return; }
            } else if fits(&self.board, &self.piece.matrix, self.piece.x, self.piece.y + 1) {
                self.piece.y += 1;
            } else { self.lock_piece(); }
        }
    }

    fn key_down(&mut self, key: &str) {
        match key {
            "ArrowLeft" | "ArrowRight" => {
                if self.das_key.as_deref() != Some(key) {
                    let dx = if key == "ArrowLeft" { -1 } else { 1 };
                    self.move_piece(dx);
                    self.das_key = Some(key.to_string()); self.das_timer = 0.0; self.das_active = false;
                }
            }
            "ArrowUp"  => self.rotate_piece(1),
            "z" | "Z"  => self.rotate_piece(-1),
            "ArrowDown" => { self.keys_down.insert(key.to_string()); }
            " "         => self.hard_drop(),
            "c" | "C"  => self.hold_piece(),
            _ => {}
        }
    }

    fn key_up(&mut self, key: &str) {
        if self.das_key.as_deref() == Some(key) {
            self.das_key = None; self.das_active = false; self.das_timer = 0.0;
        }
        self.keys_down.remove(key);
    }
}

// ── Dellacherie AI ───────────────────────────────────────────────────────────

fn ai_norm_key(shape: &[Vec<u8>]) -> Vec<i32> {
    let mut mr = i32::MAX;
    let mut mc = i32::MAX;
    for (r, row) in shape.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 { mr = mr.min(r as i32); mc = mc.min(c as i32); }
        }
    }
    let mut cells: Vec<i32> = shape.iter().enumerate()
        .flat_map(|(r, row)| row.iter().enumerate()
            .filter(|(_, &v)| v != 0)
            .map(move |(c, _)| (r as i32 - mr) * 100 + (c as i32 - mc)))
        .collect();
    cells.sort_unstable();
    cells
}

/// Returns unique rotations as (rot_index, shape) pairs.
/// rot_index is the number of CW rotations from the base — matches queue_ai_move's `rotations` arg.
fn ai_unique_rotations(piece_type: usize) -> Vec<(i32, Vec<Vec<u8>>)> {
    let base: Vec<Vec<u8>> = SHAPES[piece_type].iter().map(|r| r.to_vec()).collect();
    let mut all = vec![base];
    for _ in 0..3 { let last = all.last().unwrap().clone(); all.push(rotate_matrix(&last, 1)); }
    let mut seen = std::collections::HashSet::new();
    all.into_iter().enumerate()
        .filter(|(_, s)| seen.insert(ai_norm_key(s)))
        .map(|(i, s)| (i as i32, s))
        .collect()
}

fn ai_col_bounds(shape: &[Vec<u8>]) -> (i32, i32) {
    let mut min_c = i32::MAX;
    let mut max_c = i32::MIN;
    for row in shape {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 { min_c = min_c.min(c as i32); max_c = max_c.max(c as i32); }
        }
    }
    (min_c, max_c)
}

fn ai_can_place(board: &[u8; 200], shape: &[Vec<u8>], px: i32, py: i32) -> bool {
    for (r, row) in shape.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v == 0 { continue; }
            let br = py + r as i32;
            let bc = px + c as i32;
            if br >= ROWS as i32 || bc < 0 || bc >= COLS as i32 { return false; }
            if br >= 0 && board[br as usize * COLS + bc as usize] != 0 { return false; }
        }
    }
    true
}

fn ai_drop_y(board: &[u8; 200], shape: &[Vec<u8>], px: i32) -> i32 {
    let mut y = -(shape.len() as i32);
    while ai_can_place(board, shape, px, y + 1) { y += 1; }
    y
}

fn ai_evaluate(board: &[u8; 200], shape: &[Vec<u8>], px: i32, py: i32) -> f64 {
    let mut b = *board;
    let mut p_cells: Vec<i32> = Vec::new();
    for (r, row) in shape.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v == 0 { continue; }
            let br = py + r as i32; let bc = px + c as i32;
            if br >= 0 && br < ROWS as i32 && bc >= 0 && bc < COLS as i32 {
                b[br as usize * COLS + bc as usize] = 1;
                p_cells.push(br);
            }
        }
    }

    let mut lines_cleared = 0i32;
    let mut eroded_cells = 0i32;
    let mut r = ROWS as i32 - 1;
    while r >= 0 {
        if (0..COLS).all(|c| b[r as usize * COLS + c] != 0) {
            lines_cleared += 1;
            eroded_cells += p_cells.iter().filter(|&&pr| pr == r).count() as i32;
            for rr in (1..=r as usize).rev() {
                for c in 0..COLS { b[rr * COLS + c] = b[(rr - 1) * COLS + c]; }
            }
            for c in 0..COLS { b[c] = 0; }
        } else { r -= 1; }
    }

    let (mut min_row, mut max_row) = (i32::MAX, i32::MIN);
    for (r, row) in shape.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 { let pr = py + r as i32; let _ = c; min_row = min_row.min(pr); max_row = max_row.max(pr); }
        }
    }
    let landing_height = ROWS as f64 - (min_row + max_row) as f64 / 2.0;

    let mut row_trans = 0i32;
    for r in 0..ROWS {
        let mut prev = 1i32;
        for c in 0..COLS { let cur = if b[r * COLS + c] != 0 { 1 } else { 0 }; if cur != prev { row_trans += 1; } prev = cur; }
        if prev != 1 { row_trans += 1; }
    }

    let mut col_trans = 0i32;
    for c in 0..COLS {
        let mut prev = 0i32;
        for r in 0..ROWS { let cur = if b[r * COLS + c] != 0 { 1 } else { 0 }; if cur != prev { col_trans += 1; } prev = cur; }
        if prev != 1 { col_trans += 1; }
    }

    let mut holes = 0i32;
    for c in 0..COLS {
        let mut filled = false;
        for r in 0..ROWS { if b[r * COLS + c] != 0 { filled = true; } else if filled { holes += 1; } }
    }

    let mut well_sum = 0i32;
    for c in 0..COLS {
        let mut depth = 0i32;
        for r in 0..ROWS {
            if b[r * COLS + c] == 0 {
                let lf = c == 0 || b[r * COLS + c - 1] != 0;
                let rf = c == COLS - 1 || b[r * COLS + c + 1] != 0;
                if lf && rf { depth += 1; well_sum += depth; } else { depth = 0; }
            } else { depth = 0; }
        }
    }

    -1.0 * landing_height
    + 1.0 * (lines_cleared * eroded_cells) as f64
    + -1.0 * row_trans as f64
    + -1.0 * col_trans as f64
    + -4.0 * holes as f64
    + -1.0 * well_sum as f64
}

/// Returns [rot_index, target_x] for the best move, or [-1, -1] if none found.
#[wasm_bindgen]
pub fn best_move() -> Vec<i32> {
    let piece_type = SNAP_PIECE_TYPE.with(|t| t.get()) as usize;
    if piece_type == 0 || piece_type > 7 { return vec![-1, -1]; }
    let board = SNAP_BOARD.with(|b| *b.borrow());
    let rots = ai_unique_rotations(piece_type);
    let mut best_score = f64::NEG_INFINITY;
    let mut best_rot = -1i32;
    let mut best_x = 0i32;
    for (rot_idx, shape) in &rots {
        let (min_c, max_c) = ai_col_bounds(shape);
        for target_x in 0..(COLS as i32 - (max_c - min_c)) {
            let px = target_x - min_c;
            let py = ai_drop_y(&board, shape, px);
            if !ai_can_place(&board, shape, px, py) { continue; }
            let score = ai_evaluate(&board, shape, px, py);
            if score > best_score { best_score = score; best_rot = *rot_idx; best_x = target_x; }
        }
    }
    if best_rot < 0 { vec![-1, -1] } else { vec![best_rot, best_x] }
}

// ── Rendering ────────────────────────────────────────────────────────────────
fn draw_block(ctx: &CanvasRenderingContext2d, bx: f64, by: f64, color: &str, alpha: f64) {
    ctx.set_global_alpha(alpha);
    ctx.set_fill_style_str(color);
    ctx.fill_rect(bx, by, BS, BS);
    ctx.set_fill_style_str("rgba(255,255,255,0.22)");
    ctx.fill_rect(bx, by, BS, 3.0);
    ctx.fill_rect(bx, by, 3.0, BS);
    ctx.set_fill_style_str("rgba(0,0,0,0.38)");
    ctx.fill_rect(bx + BS - 3.0, by, 3.0, BS);
    ctx.fill_rect(bx, by + BS - 3.0, BS, 3.0);
    ctx.set_stroke_style_str("rgba(0,0,0,0.45)");
    ctx.set_line_width(1.0);
    ctx.stroke_rect(bx + 0.5, by + 0.5, BS - 1.0, BS - 1.0);
    ctx.set_global_alpha(1.0);
}

fn draw_small(ctx: &CanvasRenderingContext2d, px: f64, py: f64, sz: f64, color: &str, alpha: f64) {
    ctx.set_global_alpha(alpha);
    ctx.set_fill_style_str(color);
    ctx.fill_rect(px, py, sz, sz);
    ctx.set_fill_style_str("rgba(255,255,255,0.22)");
    ctx.fill_rect(px, py, sz, 2.0); ctx.fill_rect(px, py, 2.0, sz);
    ctx.set_fill_style_str("rgba(0,0,0,0.38)");
    ctx.fill_rect(px + sz - 2.0, py, 2.0, sz); ctx.fill_rect(px, py + sz - 2.0, sz, 2.0);
    ctx.set_global_alpha(1.0);
}

fn render_board(ctx: &CanvasRenderingContext2d, game: &Game) {
    let cw = COLS as f64 * BS; let ch = ROWS as f64 * BS;
    ctx.set_fill_style_str("#050510");
    ctx.fill_rect(0.0, 0.0, cw, ch);
    ctx.set_stroke_style_str("rgba(0,255,65,0.06)");
    ctx.set_line_width(0.5);
    for r in 0..=ROWS {
        ctx.begin_path(); ctx.move_to(0.0, r as f64 * BS); ctx.line_to(cw, r as f64 * BS); let _ = ctx.stroke();
    }
    for c in 0..=COLS {
        ctx.begin_path(); ctx.move_to(c as f64 * BS, 0.0); ctx.line_to(c as f64 * BS, ch); let _ = ctx.stroke();
    }
    for r in 0..ROWS {
        for c in 0..COLS {
            let v = game.board[r][c];
            if v != 0 { draw_block(ctx, c as f64 * BS, r as f64 * BS, COLORS[v as usize], 1.0); }
        }
    }
    if game.state != State::Playing { return; }
    let gy = ghost_row(&game.board, &game.piece);
    for (r, row) in game.piece.matrix.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 {
                draw_block(ctx, (game.piece.x + c as i32) as f64 * BS, (gy + r as i32) as f64 * BS, COLORS[v as usize], 0.18);
            }
        }
    }
    for (r, row) in game.piece.matrix.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 {
                draw_block(ctx, (game.piece.x + c as i32) as f64 * BS, (game.piece.y + r as i32) as f64 * BS, COLORS[v as usize], 1.0);
            }
        }
    }
}

fn render_preview(ctx: &CanvasRenderingContext2d, matrix: &Vec<Vec<u8>>, w: f64, h: f64, alpha: f64) {
    let sz = 22.0;
    ctx.set_fill_style_str("#050510"); ctx.fill_rect(0.0, 0.0, w, h);
    let ox = ((w - matrix[0].len() as f64 * sz) / 2.0).floor();
    let oy = ((h - matrix.len() as f64 * sz) / 2.0).floor();
    for (r, row) in matrix.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 { draw_small(ctx, ox + c as f64 * sz, oy + r as f64 * sz, sz, COLORS[v as usize], alpha); }
        }
    }
}

// ── WASM entry point ─────────────────────────────────────────────────────────
#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    let window   = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let board_canvas: HtmlCanvasElement = document.get_element_by_id("board").unwrap().dyn_into()?;
    let board_ctx: CanvasRenderingContext2d = board_canvas.get_context("2d")?.unwrap().dyn_into()?;
    let next_canvas: HtmlCanvasElement = document.get_element_by_id("next-canvas").unwrap().dyn_into()?;
    let next_ctx: CanvasRenderingContext2d = next_canvas.get_context("2d")?.unwrap().dyn_into()?;
    let hold_canvas: HtmlCanvasElement = document.get_element_by_id("hold-canvas").unwrap().dyn_into()?;
    let hold_ctx: CanvasRenderingContext2d = hold_canvas.get_context("2d")?.unwrap().dyn_into()?;

    let seed = (window.performance().unwrap().now() as u32).wrapping_add(0xDEADBEEF);
    let game = Rc::new(RefCell::new(Game::new(seed)));

    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(val)) = storage.get_item("tetris_hi") {
            if let Ok(n) = val.parse::<u32>() { game.borrow_mut().hi_score = n; }
        }
    }

    // Keyboard events
    {
        let game_kd = game.clone(); let doc_kd = document.clone();
        let closure = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            let key = e.key();
            match key.as_str() {
                "ArrowLeft"|"ArrowRight"|"ArrowUp"|"ArrowDown"|" " => { e.prevent_default(); }
                _ => {}
            }
            let mut g = game_kd.borrow_mut();
            match g.state {
                State::Menu | State::Over => {
                    if key == "Enter" {
                        g.start();
                        if let Some(el) = doc_kd.get_element_by_id("overlay") {
                            let _ = el.dyn_ref::<web_sys::HtmlElement>().map(|e| e.style().set_property("display", "none"));
                        }
                    }
                    return;
                }
                State::Paused => {
                    if key == "p" || key == "P" {
                        g.state = State::Playing; g.last_time = 0.0;
                        if let Some(el) = doc_kd.get_element_by_id("overlay") {
                            let _ = el.dyn_ref::<web_sys::HtmlElement>().map(|e| e.style().set_property("display", "none"));
                        }
                    }
                    return;
                }
                State::Playing => {}
            }
            match key.as_str() {
                "p" | "P" => {
                    g.state = State::Paused;
                    set_text_content(&doc_kd, "ol-sub", "PAUSED");
                    set_text_content(&doc_kd, "ol-msg", "PRESS P TO RESUME");
                    set_text_content(&doc_kd, "ol-score", "");
                    show_overlay(&doc_kd);
                }
                "Escape" => {
                    g.state = State::Over;
                    show_overlay_text(&doc_kd, "GAME OVER", "PRESS ENTER TO RETRY", &format!("SCORE: {}", g.score));
                }
                _ => g.key_down(&key),
            }
        });
        document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let game_ku = game.clone();
        let closure = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            game_ku.borrow_mut().key_up(&e.key());
        });
        document.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Game loop
    let game_loop = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let game_loop_init = game_loop.clone();
    let window_loop = window.clone(); let doc_loop = document.clone();

    *game_loop_init.borrow_mut() = Some(Closure::new(move |ts: f64| {
        let mut g = game.borrow_mut();

        if g.state == State::Playing { g.update(ts); }

        // Save hi-score
        if g.score > 0 {
            if let Ok(Some(storage)) = window_loop.local_storage() {
                let _ = storage.set_item("tetris_hi", &g.hi_score.to_string());
            }
        }

        // Export state for AI
        update_snapshots(&g);

        // Render
        render_board(&board_ctx, &g);
        render_preview(&next_ctx, &g.next_piece.matrix.clone(), next_canvas.width() as f64, next_canvas.height() as f64, 1.0);
        match g.held_type {
            None => {
                hold_ctx.set_fill_style_str("#050510");
                hold_ctx.fill_rect(0.0, 0.0, hold_canvas.width() as f64, hold_canvas.height() as f64);
            }
            Some(t) => {
                let m: Vec<Vec<u8>> = SHAPES[t].iter().map(|r| r.to_vec()).collect();
                render_preview(&hold_ctx, &m, hold_canvas.width() as f64, hold_canvas.height() as f64, if g.can_hold { 1.0 } else { 0.38 });
            }
        }

        update_ui(&doc_loop, &g);

        if g.state == State::Over && g.prev_state != State::Over {
            show_overlay_text(&doc_loop, "GAME OVER", "PRESS ENTER TO RETRY", &format!("SCORE: {}", g.score));
        }
        g.prev_state = g.state.clone();
        drop(g);

        window_loop.request_animation_frame(game_loop.borrow().as_ref().unwrap().as_ref().unchecked_ref()).unwrap();
    }));

    window.request_animation_frame(game_loop_init.borrow().as_ref().unwrap().as_ref().unchecked_ref())?;
    Ok(())
}

fn update_ui(doc: &web_sys::Document, g: &Game) {
    set_text_content(doc, "score-val", &g.score.to_string());
    set_text_content(doc, "hi-val",    &g.hi_score.to_string());
    set_text_content(doc, "level-val", &g.level.to_string());
    set_text_content(doc, "lines-val", &g.lines_cleared.to_string());
}

fn set_text_content(doc: &web_sys::Document, id: &str, text: &str) {
    if let Some(el) = doc.get_element_by_id(id) { el.set_text_content(Some(text)); }
}

fn show_overlay(doc: &web_sys::Document) {
    if let Some(el) = doc.get_element_by_id("overlay") {
        let _ = el.dyn_ref::<web_sys::HtmlElement>().map(|e| e.style().set_property("display", "flex"));
    }
}

fn show_overlay_text(doc: &web_sys::Document, sub: &str, msg: &str, score: &str) {
    set_text_content(doc, "ol-sub",   sub);
    set_text_content(doc, "ol-msg",   msg);
    set_text_content(doc, "ol-score", score);
    show_overlay(doc);
}
