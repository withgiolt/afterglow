#!/usr/bin/env bash
# Deploy a Worker, wait out the workers.dev 1042 window, record what its first real
# responses are (status, headers, start of body), then delete it. For a fixture that is
# expected to fail (glowvm json_roundtrip over the CPU limit).
# Usage: bench/probe-once.sh <worker-dir> [requests]   Requests: 1042 probes + requests.
set -euo pipefail
cd "$(dirname "$0")/.."
dir=$1 n=${2:-3}
name=$(grep -oE '"name": "[^"]+"' "$dir/wrangler.jsonc" | cut -d'"' -f4)
export CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000
trap 'echo "deleting $name"; npx wrangler delete --name "$name" --force 2>&1 | tail -2' EXIT
out=bench/results/$name-probe
log=$( (cd "$dir" && npx wrangler deploy 2>&1) )
version=$(grep -oE 'Current Version ID: [0-9a-f-]+' <<<"$log" | awk '{print $4}')
echo "== $name $(date -u +%FT%TZ) version $version"
for i in $(seq 1 60); do
  curl -s -D "$out.headers" -o "$out.body" -w '%{http_code} %{time_total}\n' "https://$name.SUBDOMAIN.workers.dev/" > "$out.status"
  grep -q '1042' "$out.body" || break
  sleep 15
done
echo "after $i probes:"
for r in $(seq 1 "$n"); do
  [[ $r -gt 1 ]] && curl -s -D "$out.headers" -o "$out.body" -w '%{http_code} %{time_total}\n' "https://$name.SUBDOMAIN.workers.dev/" > "$out.status"
  echo "response $r: $(cat "$out.status") body: $(head -c 200 "$out.body")"
done | tee "$out.txt"
