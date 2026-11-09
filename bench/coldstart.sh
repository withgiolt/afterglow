#!/usr/bin/env bash
# Cold start: deploy a new version, then time the very first request.
# Usage: bench/coldstart.sh <fixture-dir> <worker-name> <samples>
# Output: bench/results/<worker>-coldstart.csv
# (sample, version, startup_ms from wrangler, server_ms = TTFB - TLS, total_ms, status)
set -euo pipefail
cd "$(dirname "$0")/.."
dir=$1 name=$2 n=$3
out=bench/results/$name-coldstart.csv
echo "sample,version,startup_ms,server_ms,total_ms,status" > "$out"
for i in $(seq 1 "$n"); do
  log=$( (cd "$dir" && npx wrangler deploy 2>&1) )
  startup=$(grep -oE 'Worker Startup Time: [0-9]+' <<<"$log" | grep -oE '[0-9]+$' || echo "")
  version=$(grep -oE 'Current Version ID: [0-9a-f-]+' <<<"$log" | awk '{print $4}')
  sleep 5
  read -r status appconnect ttfb total < <(curl -s -o /dev/null -w '%{http_code} %{time_appconnect} %{time_starttransfer} %{time_total}\n' "https://$name.SUBDOMAIN.workers.dev/")
  server=$(awk -v a="$appconnect" -v t="$ttfb" 'BEGIN{printf "%.1f", (t-a)*1000}')
  tot=$(awk -v t="$total" 'BEGIN{printf "%.1f", t*1000}')
  echo "$i,$version,$startup,$server,$tot,$status" | tee -a "$out"
done
