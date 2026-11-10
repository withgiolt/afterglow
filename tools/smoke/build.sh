#!/usr/bin/env bash
# build.sh [FIXTURE...]: builds the glowc Worker for each fixture into out/workers/<fixture>
# (default: every fixture tests/smoke.test.ts and tools/smoke/run.sh use).
set -euo pipefail
cd "$(dirname "$0")/../.."
F=("$@")
[[ ${#F[@]} == 0 ]] && F=(basic_get not_found env_get query_param bytes_body echo_body no_content lustre_html file_body json_roundtrip $(ls fixtures/app/src | sed -n 's/^\(stdlib_[a-z_]*\)\.gleam$/\1/p'))
for f in "${F[@]}"; do tools/build-worker.sh "$f" "out/workers/$f" 2>&1 | grep -v " warnings (-v to list)" | sed "s/^/[$f] /" | tail -1; done
