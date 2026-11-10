#!/usr/bin/env bash
# Cold start: deploy a new version, then time the very first request.
# Usage: bench/coldstart.sh <fixture-dir> <worker-name> <samples>
# Output: bench/results/<worker>-coldstart.csv
# (sample, version, startup_ms from wrangler, server_ms = TTFB - TLS, total_ms, status)
set -euo pipefail
if [[ -z "${CLOUDFLARE_ACCOUNT_ID:-}" || -z "${CF_WORKERS_SUBDOMAIN:-}" ]]; then
  echo "set CLOUDFLARE_ACCOUNT_ID and CF_WORKERS_SUBDOMAIN (<name>.<subdomain>.workers.dev)" >&2; exit 1
fi
cd "$(dirname "$0")/.."
dir=$1 name=$2 n=$3
out=bench/results/$name-coldstart.csv
echo "sample,version,startup_ms,server_ms,total_ms,status" > "$out"
for i in $(seq 1 "$n"); do
  log=$( (cd "$dir" && npx wrangler deploy 2>&1) )
  startup=$(grep -oE 'Worker Startup Time: [0-9]+' <<<"$log" | grep -oE '[0-9]+$' || echo "")
  version=$(grep -oE 'Current Version ID: [0-9a-f-]+' <<<"$log" | awk '{print $4}')
  # A new version takes a minute or more to reach every edge (an older version served
  # whole cold-start series at a 5 s wait): wait, then check the version in analytics.
  sleep "${COLD_WAIT:-60}"
  read -r status appconnect ttfb total < <(curl -s -o /dev/null -w '%{http_code} %{time_appconnect} %{time_starttransfer} %{time_total}\n' "https://$name.$CF_WORKERS_SUBDOMAIN.workers.dev/")
  server=$(awk -v a="$appconnect" -v t="$ttfb" 'BEGIN{printf "%.1f", (t-a)*1000}')
  tot=$(awk -v t="$total" 'BEGIN{printf "%.1f", t*1000}')
  echo "$i,$version,$startup,$server,$tot,$status" | tee -a "$out"
done
