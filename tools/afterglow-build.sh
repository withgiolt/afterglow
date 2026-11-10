#!/usr/bin/env bash
# afterglow-build.sh OUT.wasm --entry M:F/0 [--beams DIR]... [afterglow options]
# Builds afterglow_runtime and afterglow, compiles, merges with afterglow_runtime, optimises.
#   AFTERGLOW_PREBUILT=1   skip the cargo builds (the caller built afterglow and afterglow_runtime once)
#   AFTERGLOW_OTP_LIBS     newline-separated OTP ebin dirs (the caller looked them up once)
#   AFTERGLOW_NOOPT=1      skip wasm-opt (tests: correctness doesn't depend on it)
#   AFTERGLOW_NAMES=1      keep function names (profiling)
set -euo pipefail
cd "$(dirname "$0")/.."
out=$1; shift
if [[ "${AFTERGLOW_PREBUILT:-}" != 1 ]]; then
  cargo build -q --release -p afterglow
  cargo build -q --release -p afterglow_runtime --target wasm32-unknown-unknown
fi
rt=target/wasm32-unknown-unknown/release/afterglow_runtime.wasm
libs=()
if [[ -n "${AFTERGLOW_OTP_LIBS:-}" ]]; then
  while IFS= read -r d; do [[ -n "$d" ]] && libs+=(--beams "$d"); done <<<"$AFTERGLOW_OTP_LIBS"
else
  while IFS= read -r d; do libs+=(--beams "$d"); done < <(tools/otp-libs.sh)
fi
tmp=$(mktemp -d)
target/release/afterglow build --rt "$rt" "$@" "${libs[@]}" -o "$tmp/app.wasm"
feat=(--enable-tail-call --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue)
wasm-merge "${feat[@]}" $([[ "${AFTERGLOW_NAMES:-}" == 1 ]] && echo -g) "$rt" afterglow_runtime "$tmp/app.wasm" app -o "$tmp/merged.wasm"
if [[ "${AFTERGLOW_NOOPT:-}" == 1 ]]; then cp "$tmp/merged.wasm" "$out"; else wasm-opt "${feat[@]}" -O3 $([[ "${AFTERGLOW_NAMES:-}" == 1 ]] && echo -g) "$tmp/merged.wasm" -o "$out"; fi
rm -rf "$tmp"
