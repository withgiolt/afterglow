#!/usr/bin/env bash
# run.sh [FIXTURE...]: builds afterglow Workers, serves every fixture from one workerd,
# runs check.mjs, stops workerd.
set -euo pipefail
cd "$(dirname "$0")/../.."
F=("$@")
[[ ${#F[@]} == 0 ]] && F=(basic_get not_found env_get query_param bytes_body echo_body no_content lustre_html file_body json_roundtrip missing_function $(ls fixtures/app/src | sed -n 's/^\(stdlib_[a-z_]*\)\.gleam$/\1/p'))
if [[ "${SKIP_BUILD:-}" != 1 ]]; then
  tools/smoke/build.sh "${F[@]}"
fi
node tools/smoke/gen-config.mjs out/smoke.capnp "${F[@]}"
node_modules/.bin/workerd serve out/smoke.capnp --experimental >out/smoke.workerd.log 2>&1 &
wpid=$!
trap 'kill $wpid 2>/dev/null || true' EXIT
for i in $(seq 1 100); do curl -s -o /dev/null "http://127.0.0.1:$((9400 + ${#F[@]} - 1))/" && break; sleep 0.2; done
node tools/smoke/check.mjs "${F[@]}"
