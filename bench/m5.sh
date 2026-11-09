#!/usr/bin/env bash
# M5, one deployed Worker: 5 cold-start deploys, then 3 Phase 0 runs on the last version,
# then delete the Worker (also on failure).
# Usage: bench/m5.sh <worker-dir> [runs]   (the name comes from wrangler.jsonc)
# Requests: 5 + runs * 6,000.
set -euo pipefail
cd "$(dirname "$0")/.."
dir=$1 runs=${2:-3}
name=$(grep -oE '"name": "[^"]+"' "$dir/wrangler.jsonc" | cut -d'"' -f4)
export CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000
trap 'echo "deleting $name"; npx wrangler delete --name "$name" --force 2>&1 | tail -2' EXIT
echo "== $name $(date -u +%FT%TZ)"
bench/coldstart.sh "$dir" "$name" 5
version=$(tail -1 "bench/results/$name-coldstart.csv" | cut -d, -f2)
for r in $(seq 1 "$runs"); do
  echo "-- run $r $(date -u +%FT%TZ)"
  bench/measure.sh "$name" "r$r" "$version"
done
node bench/coldcpu.mjs "bench/results/$name-coldstart.csv"
