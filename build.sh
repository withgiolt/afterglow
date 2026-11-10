#!/usr/bin/env bash
# Assemble every wat/*.wat, run it through wasm-opt with the flags afterglow will use,
# and validate the wasm-opt output (the bytes workerd actually loads).
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p build
FEATURES=(--enable-tail-call --enable-bulk-memory --enable-multivalue --enable-sign-ext --enable-nontrapping-float-to-int)
for src in wat/*.wat; do
  name="$(basename "$src" .wat)"
  extra=()
  vflags=()
  case "$name" in
    eh_exnref) extra=(--enable-exception-handling) ;;
    eh_legacy) extra=(--enable-exception-handling); vflags=(--features=all) ;;
  esac
  wasm-tools parse "$src" -o "build/$name.raw.wasm"
  # $name.wasm: Binaryen re-encode only, so the probe's instructions survive as written.
  # $name.oz.wasm: the -Oz build afterglow will ship. Both are loaded in workerd.
  wasm-opt "${FEATURES[@]}" ${extra[@]+"${extra[@]}"} --strip-debug --strip-producers \
    "build/$name.raw.wasm" -o "build/$name.wasm"
  wasm-opt "${FEATURES[@]}" ${extra[@]+"${extra[@]}"} -Oz --strip-debug --strip-producers \
    "build/$name.raw.wasm" -o "build/$name.oz.wasm"
  for out in "build/$name.wasm" "build/$name.oz.wasm"; do
    wasm-tools validate ${vflags[@]+"${vflags[@]}"} "$out"
    echo "$out: $(wc -c < "$out" | tr -d ' ') bytes, valid"
  done
done
