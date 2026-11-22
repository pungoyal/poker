#!/bin/sh
# Builds the browser app into web/www: compiles the engine to WebAssembly and
# generates its JavaScript bindings next to the static files. Serve web/www
# with any static file server afterwards.
#
# Needs the wasm32 target and a wasm-bindgen CLI matching web/Cargo.toml:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.130 --locked
set -eu
cd "$(dirname "$0")/.."
cargo build --package poker-web --release --locked --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/www/pkg \
    target/wasm32-unknown-unknown/release/poker_web.wasm
echo "Built web/www. Try: python3 -m http.server --directory web/www 8080"
