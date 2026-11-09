#!/usr/bin/env bash
# One Phase 0 run against a deployed Worker, CPU from Workers Analytics.
# Usage: bench/measure.sh <worker-name> <run-label> <scriptVersion> [path]
# 1,000 warm-up requests at c=10, then 5,000 measured at c=10, 25 req/s; then
# bench/analytics.mjs over the measured window only.
set -euo pipefail
cd "$(dirname "$0")/.."
name=$1 label=$2 version=$3 path=${4:-/}
url="https://$name.SUBDOMAIN.workers.dev$path"
out=bench/results/$name-$label
oha -n 1000 -c 10 --no-tui --output-format json "$url" > "$out.warm.oha.json"
sleep 5
oha -n 5000 -c 10 -q 25 --no-tui --output-format json "$url" > "$out.oha.json"
node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1]));console.log("status", JSON.stringify(r.statusCodeDistribution), "errors", JSON.stringify(r.errorDistribution))' "$out.oha.json"
sleep 90 # analytics ingestion delay
node bench/analytics.mjs "$out" "$version" || echo "analytics failed: requery later with node bench/analytics.mjs $out $version"
