#!/usr/bin/env bash
# glowc-build.sh OUT.wasm --entry M:F/0 [--beams DIR]... [glowc options]
# Builds glowrt and glowc, compiles, merges with glowrt, optimises.
#   GLOWC_PREBUILT=1   skip the cargo builds (the caller built glowc and glowrt once)
#   GLOWC_OTP_LIBS     newline-separated OTP ebin dirs (the caller looked them up once)
#   GLOWC_NOOPT=1      skip wasm-opt (tests: correctness doesn't depend on it)
#   GLOWC_NAMES=1      keep function names (profiling)
set -euo pipefail
cd "$(dirname "$0")/.."
out=$1; shift
if [[ "${GLOWC_PREBUILT:-}" != 1 ]]; then
  cargo build -q --release -p glowc
  cargo build -q --release -p glowrt --target wasm32-unknown-unknown
fi
rt=target/wasm32-unknown-unknown/release/glowrt.wasm
libs=()
if [[ -n "${GLOWC_OTP_LIBS:-}" ]]; then
  while IFS= read -r d; do [[ -n "$d" ]] && libs+=(--beams "$d"); done <<<"$GLOWC_OTP_LIBS"
else
  while IFS= read -r d; do libs+=(--beams "$d"); done < <(tools/otp-libs.sh)
fi
tmp=$(mktemp -d)
target/release/glowc build --rt "$rt" "$@" "${libs[@]}" -o "$tmp/app.wasm"
feat=(--enable-tail-call --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue)
wasm-merge "${feat[@]}" $([[ "${GLOWC_NAMES:-}" == 1 ]] && echo -g) "$rt" glowrt "$tmp/app.wasm" app -o "$tmp/merged.wasm"
if [[ "${GLOWC_NOOPT:-}" == 1 ]]; then cp "$tmp/merged.wasm" "$out"; else wasm-opt "${feat[@]}" -O3 $([[ "${GLOWC_NAMES:-}" == 1 ]] && echo -g) "$tmp/merged.wasm" -o "$out"; fi
rm -rf "$tmp"
