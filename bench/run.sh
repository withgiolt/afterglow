#!/usr/bin/env bash
# One benchmark run against a deployed Worker.
# Usage: bench/run.sh <worker-name> <run-label>
# 1,000 warm-up requests (not tailed), then 5,000 measured at concurrency 10,
# rate-limited to 25 req/s so wrangler tail does not sample (it kept 66 of 10,000 at ~330 req/s)
# while `wrangler tail --format json` records per-invocation cpuTime/wallTime.
set -euo pipefail
if [[ -z "${CLOUDFLARE_ACCOUNT_ID:-}" || -z "${CF_WORKERS_SUBDOMAIN:-}" ]]; then
  echo "set CLOUDFLARE_ACCOUNT_ID and CF_WORKERS_SUBDOMAIN (<name>.<subdomain>.workers.dev)" >&2; exit 1
fi
cd "$(dirname "$0")/.."
name=$1 label=$2
url="https://$name.$CF_WORKERS_SUBDOMAIN.workers.dev/"
out=bench/results/$name-$label

oha -n 1000 -c 10 --no-tui --output-format json "$url" > "$out.warm.oha.json"

npx wrangler tail "$name" --format json > "$out.tail.json" 2> "$out.tail.err" &
sleep 10
oha -n 5000 -c 10 -q 25 --no-tui --output-format json "$url" > "$out.oha.json"
sleep 15
pkill -f "wrangler tail $name --format json" || true
node bench/strip-tail.mjs "$out.tail.json"
node bench/summarize.mjs "$out"
