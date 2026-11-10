#!/usr/bin/env bash
# local.sh FIXTURE... : afterglow under a local workerd, latency per request with oha at
# concurrency 1 (pure compute, so wall time is close to CPU time).
# 200 warm-up + N measured (default 2000) per fixture.
set -euo pipefail
cd "$(dirname "$0")/.."
N=${N:-2000}
F=("$@")
node tools/smoke/gen-config.mjs out/local.capnp "${F[@]}"
node_modules/.bin/workerd serve out/local.capnp >out/local.workerd.log 2>&1 &
wpid=$!
trap 'kill $wpid 2>/dev/null || true' EXIT
for i in $(seq 1 100); do curl -s -o /dev/null "http://127.0.0.1:$((9400 + ${#F[@]} - 1))/" && break; sleep 0.2; done
for i in "${!F[@]}"; do
  port=$((9400 + i))
  oha -n $((N / 10 + 1)) -c 1 --no-tui "http://127.0.0.1:$port/" >/dev/null
  oha -n $N -c 1 --no-tui --output-format json "http://127.0.0.1:$port/" > out/local-${F[$i]}.json
  node -e '
    const r = JSON.parse(require("fs").readFileSync(process.argv[1]));
    const p = r.latencyPercentiles, ms = (s) => (s * 1000).toFixed(2);
    console.log(`${process.argv[2].padEnd(15)} n=${process.argv[3]} p50 ${ms(p.p50)} ms  p99 ${ms(p.p99)} ms  ok=${JSON.stringify(r.statusCodeDistribution)}`);
  ' out/local-${F[$i]}.json ${F[$i]} $N
done
