#!/usr/bin/env bash
# Deployed checks for P2-P5 (each Worker deleted after its check, also on failure).
# Usage: tools/do/deployed.sh p2|p3|p4|p5      Requests: tens per check.
# p3-p5 deploy out/do-pN, which tools/do/test.sh builds. p2 deploys out/do-v1 and
# out/do-v2: after tools/p2-test.sh, run
# `tools/do/build-do.sh out/p2vN out/do-vN afterglow-p2-counter counter` for N = 1, 2.
set -uo pipefail
cd "$(dirname "$0")/../.."
# No `sed -i`: GNU and BSD sed disagree on its argument.
dev() {
  local c=$1/wrangler.jsonc
  grep -q workers_dev "$c" && return
  sed 's/  "compatibility_date"/  "workers_dev": true, "compatibility_date"/' "$c" >"$c.tmp" && mv "$c.tmp" "$c"
}
deploy() { dev "$1"; (cd "$1" && npx wrangler deploy 2>&1) | grep -E 'Startup|Version ID|Uploaded' ; }
ready() { # a new workers.dev hostname answers 1042 for a few minutes
  for i in $(seq 1 60); do
    b=$(curl -s "https://$1.$CF_WORKERS_SUBDOMAIN.workers.dev/?node=probe"); grep -q 1042 <<<"$b" || break; sleep 15
  done; echo "ready after $i probes"
}
base() { echo "https://$1.$CF_WORKERS_SUBDOMAIN.workers.dev"; }
name=""
trap '[[ -n $name ]] && { echo "deleting $name"; npx wrangler delete --name "$name" --force 2>&1 | tail -1; }' EXIT
case $1 in
p2) name=afterglow-p2-counter
  deploy out/do-v1; ready $name
  node tools/do/p2-deployed.mjs "$(base $name)" incr incr
  echo "idle 180 s (DO eviction)"; sleep 180
  node tools/do/p2-deployed.mjs "$(base $name)" incr
  deploy out/do-v2; echo "wait 90 s for the new version"; sleep 90
  node tools/do/p2-deployed.mjs "$(base $name)" incr get
  echo "idle 180 s"; sleep 180
  node tools/do/p2-deployed.mjs "$(base $name)" incr ;;
p3) name=afterglow-p3-pingpong
  deploy out/do-p3; ready $name
  node tools/do/p3-test.mjs out/do-p3/wrangler.jsonc "${P3_N:-200}" "$(base $name)" ;;
p4) name=afterglow-p4-timers
  deploy out/do-p4; ready $name
  node tools/do/p4-deployed.mjs "$(base $name)" arm 60000
  echo "idle ${P4_IDLE:-200} s, no requests: only the alarm can wake the DO; cron runs each minute"; sleep "${P4_IDLE:-200}"
  node tools/do/p4-deployed.mjs "$(base $name)" check ;;
p5) name=afterglow-p5-chat
  deploy out/do-p5; ready $name
  node tools/do/p5-test.mjs out/do-p5/wrangler.jsonc "wss://$name.$CF_WORKERS_SUBDOMAIN.workers.dev" 30000 ;;
esac
