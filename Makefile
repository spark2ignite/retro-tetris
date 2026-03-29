.PHONY: wasm server run

wasm:
	wasm-pack build tetris-wasm --target web --out-dir ../web/pkg

server: wasm
	cargo build --release -p tetris-server

run: server
	./target/release/tetris-server
