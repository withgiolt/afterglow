#!/usr/bin/env bash
# at-utc.sh <ISO-UTC> <cmd...>: wait until the given UTC time, then run cmd.
set -euo pipefail
t=$(date -j -u -f "%Y-%m-%dT%H:%M:%SZ" "$1" +%s); shift
while (( $(date -u +%s) < t )); do sleep 60; done
exec "$@"
