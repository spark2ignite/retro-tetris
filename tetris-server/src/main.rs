use tiny_http::{Header, Response, Server};

// All game assets embedded at compile time — no files needed at runtime
const HTML: &[u8] = include_bytes!("../index.html");
const JS:   &[u8] = include_bytes!("../pkg/tetris_wasm.js");
const WASM: &[u8] = include_bytes!("../pkg/tetris_wasm_bg.wasm");

fn main() {
    let addr = "127.0.0.1:8765";
    let server = Server::http(addr).expect("Failed to start server");

    println!("TETRIS running at http://localhost:8765  (Ctrl+C to quit)");

    // Open the browser automatically
    if let Err(e) = open::that("http://localhost:8765") {
        eprintln!("Could not open browser automatically: {e}");
        println!("Open http://localhost:8765 manually.");
    }

    for request in server.incoming_requests() {
        let url = request.url().to_string();

        let (body, mime) = match url.trim_end_matches('/') {
            "" | "/index.html" =>
                (HTML, "text/html; charset=utf-8"),
            "/pkg/tetris_wasm.js" =>
                (JS,   "application/javascript; charset=utf-8"),
            "/pkg/tetris_wasm_bg.wasm" =>
                (WASM, "application/wasm"),
            _ => {
                let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
                continue;
            }
        };

        let ct = Header::from_bytes("Content-Type", mime).unwrap();
        let _ = request.respond(Response::from_data(body).with_header(ct));
    }
}
