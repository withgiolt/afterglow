#!/usr/bin/env bash
# local.sh FIXTURE... : glowvm vs glowc under one local workerd, latency per request with
# oha at concurrency 1 (pure compute, so wall time is close to CPU time).
# 200 warm-up + N measured (default 2000) per runtime and fixture.
set -euo pipefail
cd "$(dirname "$0")/.."
N=${N:-2000}
F=("$@")
node tools/smoke/gen-config.mjs out/local.capnp "${F[@]}"
node_modules/.bin/workerd serve out/local.capnp >out/local.workerd.log 2>&1 &
wpid=$!
trap 'kill $wpid 2>/dev/null || true' EXIT
for i in $(seq 1 100); do curl -s -o /dev/null "http://127.0.0.1:$((9500 + ${#F[@]} - 1))/" && break; sleep 0.2; done
for i in "${!F[@]}"; do
  for rt in vm gc; do
    [[ $rt == vm ]] && port=$((9400 + i)) || port=$((9500 + i))
    n=$N; [[ $rt == vm && ${F[$i]} == json_roundtrip ]] && n=${NJSON_VM:-20}
    oha -n $((n / 10 + 1)) -c 1 --no-tui "http://127.0.0.1:$port/" >/dev/null
    oha -n $n -c 1 --no-tui --output-format json "http://127.0.0.1:$port/" > out/local-$rt-${F[$i]}.json
    node -e '
      const r = JSON.parse(require("fs").readFileSync(process.argv[1]));
      const p = r.latencyPercentiles, ms = (s) => (s * 1000).toFixed(2);
      console.log(`${process.argv[2].padEnd(6)} ${process.argv[3].padEnd(15)} n=${process.argv[4]} p50 ${ms(p.p50)} ms  p99 ${ms(p.p99)} ms  ok=${JSON.stringify(r.statusCodeDistribution)}`);
    ' out/local-$rt-${F[$i]}.json $([[ $rt == vm ]] && echo glowvm || echo glowc) ${F[$i]} $n
  done
done
