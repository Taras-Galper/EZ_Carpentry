#!/usr/bin/env bash
# Builds the CLI, the WASM module, a pure-JS fallback and the single-file web app (web/dist/index.html).
# Needs: Rust stable + `rustup target add wasm32-unknown-unknown` + `rustup component add rust-src`,
#        `cargo install wasm-bindgen-cli --version 0.2.100`, Node 18+.
# BUILD_STD=1 also rebuilds std for the main wasm build (when the wasm32 target can't be installed).
set -euo pipefail
cd "$(dirname "$0")"
T=wasm32-unknown-unknown
cargo test -p casework-core
cargo build --release -p casework
STD="-Zbuild-std=std,panic_abort"
if [ "${BUILD_STD:-}" = 1 ]; then
  RUSTC_BOOTSTRAP=1 cargo build --release -p casework-wasm --target $T $STD
else
  cargo build --release -p casework-wasm --target $T
fi
wasm-bindgen --target web --out-dir web/pkg target/$T/release/casework_wasm.wasm
# Pure-JS fallback (for hosts whose CSP blocks WebAssembly): MVP features only, then wasm2js.
# std is rebuilt too, so no post-MVP instructions or externref tables leak in.
RUSTFLAGS="-C target-cpu=mvp" RUSTC_BOOTSTRAP=1 cargo build --release -p casework-wasm --target $T --target-dir target-mvp $STD
wasm-bindgen --target bundler --out-dir web/pkgjs target-mvp/$T/release/casework_wasm.wasm
(cd web && npm install --silent)
web/node_modules/binaryen/bin/wasm2js -O2 web/pkgjs/casework_wasm_bg.wasm -o web/pkgjs/casework_wasm_bg.wasm.js
sed -i 's|"./casework_wasm_bg.wasm"|"./casework_wasm_bg.wasm.js"|' web/pkgjs/casework_wasm.js
# wasm2js memory never detaches on grow; refresh views on buffer change instead.
sed -i 's|cachedUint8ArrayMemory0.byteLength === 0)|cachedUint8ArrayMemory0.buffer !== wasm.memory.buffer)|; s|cachedDataViewMemory0.buffer.detached === true \|\| (cachedDataViewMemory0.buffer.detached === undefined \&\& cachedDataViewMemory0.buffer !== wasm.memory.buffer)|cachedDataViewMemory0.buffer !== wasm.memory.buffer|' web/pkgjs/casework_wasm_bg.js
(cd web && node build.mjs)
