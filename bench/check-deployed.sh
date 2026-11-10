#!/usr/bin/env bash
# Correctness on the deployed Worker: one GET, status + headers + body compared with a
# reference captured the same way (curl -sD), ignoring per-request edge headers.
# Usage: bench/check-deployed.sh <worker-name> <ref-prefix>  (ref-prefix.headers/.body)
set -euo pipefail
if [[ -z "${CLOUDFLARE_ACCOUNT_ID:-}" || -z "${CF_WORKERS_SUBDOMAIN:-}" ]]; then
  echo "set CLOUDFLARE_ACCOUNT_ID and CF_WORKERS_SUBDOMAIN (<name>.<subdomain>.workers.dev)" >&2; exit 1
fi
cd "$(dirname "$0")/.."
name=$1 ref=$2
out=bench/results/$name
curl -s -D "$out.headers" -o "$out.body" "https://$name.$CF_WORKERS_SUBDOMAIN.workers.dev/"
filt() { grep -viE '^(date|report-to|nel|server|cf-ray|alt-svc):' "$1" | tr -d '\r'; }
if diff <(filt "$ref.headers") <(filt "$out.headers") && cmp "$ref.body" "$out.body"; then
  echo "deployed correctness: identical to $ref"
else
  echo "deployed correctness: DIFFERS from $ref"
fi
