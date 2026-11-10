#!/usr/bin/env bash
# build-worker.sh FIXTURE OUTDIR: an afterglow Worker (app.wasm + index.js) for FIXTURE:start/0.
set -euo pipefail
cd "$(dirname "$0")/.."
mod=$1 out=$2
eb=fixtures/app/build/dev/erlang
args=()
for d in $eb/*/ebin; do args+=(--beams "$d"); done
mkdir -p "$out"
tools/afterglow-build.sh "$out/app.wasm" --entry "$mod:start/0" "${args[@]}"
cp tools/worker/index.js "$out/index.js"
