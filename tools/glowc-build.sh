#!/usr/bin/env bash
# glowc-build.sh OUT.wasm --entry M:F/0 [--beams DIR]... [glowc options]
# Builds glowrt and glowc, compiles, merges with glowrt, optimises.
set -euo pipefail
cd "$(dirname "$0")/.."
out=$1; shift
cargo build -q --release -p glowc
cargo build -q --release -p glowrt --target wasm32-unknown-unknown
rt=target/wasm32-unknown-unknown/release/glowrt.wasm
otp=$(erl -noshell -eval 'io:put_chars(code:root_dir()), halt().')
libs=()
for d in "$otp"/lib/stdlib-*/ebin "$otp"/lib/kernel-*/ebin "$otp"/lib/erts-*/ebin; do libs+=(--beams "$d"); done
tmp=$(mktemp -d)
target/release/glowc build --rt "$rt" "$@" "${libs[@]}" -o "$tmp/app.wasm"
feat=(--enable-tail-call --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue)
wasm-merge "${feat[@]}" $([[ "${GLOWC_NAMES:-}" == 1 ]] && echo -g) "$rt" glowrt "$tmp/app.wasm" app -o "$tmp/merged.wasm"
if [[ "${GLOWC_NOOPT:-}" == 1 ]]; then cp "$tmp/merged.wasm" "$out"; else wasm-opt "${feat[@]}" -O3 $([[ "${GLOWC_NAMES:-}" == 1 ]] && echo -g) "$tmp/merged.wasm" -o "$out"; fi
rm -rf "$tmp"
