#!/usr/bin/env bash
# build-project.sh PROJECT_DIR MODULE OUTDIR [afterglow options]: an afterglow Worker (app.wasm,
# index.js and host.js) for MODULE:start/0 of an already built Gleam project (`gleam build`).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
proj=$(cd "$1" && pwd) mod=$2 out=$3; shift 3
args=()
for d in "$proj"/build/dev/erlang/*/ebin; do args+=(--beams "$d"); done
mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$here"
tools/afterglow-build.sh "$out/app.wasm" --entry "$mod:start/0" "${args[@]}" "$@"
cp tools/worker/index.js tools/worker/host.js "$out/"
