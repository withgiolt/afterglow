#!/usr/bin/env bash
# build.sh [FIXTURE...]: builds the glowc Worker for each fixture into out/workers/<fixture>
# (default: every fixture tests/smoke.test.ts and tools/smoke/run.sh use), JOBS at a time.
# These are the deployable builds, so they keep wasm-opt -O3.
set -euo pipefail
cd "$(dirname "$0")/../.."
F=("$@")
[[ ${#F[@]} == 0 ]] && F=(basic_get not_found env_get query_param bytes_body echo_body no_content lustre_html file_body json_roundtrip $(ls fixtures/app/src | sed -n 's/^\(stdlib_[a-z_]*\)\.gleam$/\1/p'))
cargo build -q --release -p glowc 2>/dev/null
cargo build -q --release -p glowrt --target wasm32-unknown-unknown 2>/dev/null
export GLOWC_PREBUILT=1
export GLOWC_OTP_LIBS; GLOWC_OTP_LIBS=$(tools/otp-libs.sh)
jobs_max=${JOBS:-$(nproc 2>/dev/null || echo 4)}
for f in "${F[@]}"; do
  while (( $(jobs -rp | wc -l) >= jobs_max )); do wait -n; done
  ( tools/build-worker.sh "$f" "out/workers/$f" 2>&1 | grep -v " warnings (-v to list)" | sed "s/^/[$f] /" | tail -1 ) &
done
wait
