#!/usr/bin/env bash
# Build the browser cdylib and generate its ES-module bindings.
set -euo pipefail
cd "$(dirname "$0")/.."
bindgen_version=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
if ! command -v wasm-bindgen >/dev/null; then
    echo "Install the matching binding generator: cargo install wasm-bindgen-cli --version $bindgen_version --locked" >&2
    exit 1
fi
if [[ "$(wasm-bindgen --version)" != "wasm-bindgen $bindgen_version" ]]; then
    echo "wasm-bindgen-cli must match Cargo.lock ($bindgen_version)." >&2
    exit 1
fi
cargo build --locked --release --target wasm32-unknown-unknown --lib
wasm-bindgen "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/fleece.wasm" \
    --target web --out-dir web/pkg --out-name fleece
printf '%s\n' 'Built web/. Serve with: python3 -m http.server 8080 --directory web'
