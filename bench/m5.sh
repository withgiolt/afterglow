#!/usr/bin/env bash
# M5, one deployed Worker: 5 cold-start deploys, then 3 Phase 0 runs on the last version,
# then delete the Worker (also on failure).
# Usage: bench/m5.sh <worker-dir> [runs] [ref-prefix]   (the name comes from wrangler.jsonc)
# Requests: 5 + 1 (correctness, if ref-prefix) + runs * 6,000.
set -euo pipefail
cd "$(dirname "$0")/.."
dir=$1 runs=${2:-3} ref=${3:-}
name=$(grep -oE '"name": "[^"]+"' "$dir/wrangler.jsonc" | cut -d'"' -f4)
export CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000
trap 'echo "deleting $name"; npx wrangler delete --name "$name" --force 2>&1 | tail -2' EXIT
echo "== $name $(date -u +%FT%TZ)"
# A new workers.dev hostname takes minutes to serve (Cloudflare error 1042 until then):
# deploy once and wait for a 200 before any measured request (one probe per 15 s).
(cd "$dir" && npx wrangler deploy >/dev/null 2>&1)
for i in $(seq 1 60); do
  code=$(curl -s -o /dev/null -w '%{http_code}' "https://$name.SUBDOMAIN.workers.dev/")
  [[ $code == 200 ]] && break
  sleep 15
done
echo "ready after $i probes (last status $code)"
[[ $code == 200 ]] || { echo "not serving; giving up"; exit 1; }
bench/coldstart.sh "$dir" "$name" 5
if grep -qv ',200$' <(tail -n +2 "bench/results/$name-coldstart.csv"); then echo "a cold-start request was not 200; no load"; exit 1; fi
[[ -n $ref ]] && bench/check-deployed.sh "$name" "$ref"
version=$(tail -1 "bench/results/$name-coldstart.csv" | cut -d, -f2)
for r in $(seq 1 "$runs"); do
  echo "-- run $r $(date -u +%FT%TZ)"
  bench/measure.sh "$name" "r$r" "$version"
done
node bench/coldcpu.mjs "bench/results/$name-coldstart.csv" || echo "coldcpu failed: requery later"
