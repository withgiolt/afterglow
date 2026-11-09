#!/usr/bin/env bash
# run.sh [FIXTURE...]: builds glowc Workers, starts one workerd with every fixture on
# both runtimes, runs compare.mjs, stops workerd.
set -euo pipefail
cd "$(dirname "$0")/../.."
F=("$@")
[[ ${#F[@]} == 0 ]] && F=(basic_get not_found env_get query_param bytes_body echo_body no_content lustre_html file_body json_roundtrip $(ls fixtures/app/src | sed -n 's/^\(stdlib_[a-z_]*\)\.gleam$/\1/p'))
if [[ "${SKIP_BUILD:-}" != 1 ]]; then
  for f in "${F[@]}"; do tools/build-worker.sh "$f" "out/workers/$f" 2>&1 | grep -v " warnings (-v to list)" | sed "s/^/[$f] /" | tail -1; done
fi
node tools/smoke/gen-config.mjs out/smoke.capnp "${F[@]}"
node_modules/.bin/workerd serve out/smoke.capnp --experimental >out/smoke.workerd.log 2>&1 &
wpid=$!
trap 'kill $wpid 2>/dev/null || true' EXIT
for i in $(seq 1 100); do curl -s -o /dev/null "http://127.0.0.1:$((9500 + ${#F[@]} - 1))/" && break; sleep 0.2; done
node tools/smoke/compare.mjs "${F[@]}"
