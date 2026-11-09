#!/usr/bin/env bash
# Correctness on the deployed Worker: one GET, status + headers + body compared with a
# reference captured the same way (curl -sD), ignoring per-request edge headers.
# Usage: bench/check-deployed.sh <worker-name> <ref-prefix>  (ref-prefix.headers/.body)
set -euo pipefail
cd "$(dirname "$0")/.."
name=$1 ref=$2
out=bench/results/$name
curl -s -D "$out.headers" -o "$out.body" "https://$name.SUBDOMAIN.workers.dev/"
filt() { grep -viE '^(date|report-to|nel|server|cf-ray|alt-svc):' "$1" | tr -d '\r'; }
if diff <(filt "$ref.headers") <(filt "$out.headers") && cmp "$ref.body" "$out.body"; then
  echo "deployed correctness: identical to $ref"
else
  echo "deployed correctness: DIFFERS from $ref"
fi
