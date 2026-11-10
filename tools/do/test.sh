#!/usr/bin/env bash
# test.sh [p3|p4|p5]...: builds the P3-P5 Durable Object Workers into out/do-pN (the
# directories tools/do/deployed.sh deploys) and runs their local workerd tests.
# P4_MS is P4's timer (default 5000; p4-test.mjs alone defaults to the deployed 60 s).
set -euo pipefail
cd "$(dirname "$0")/../.."
build() { # build N MODULE NAME
  mkdir -p "out/p$1"
  erlc -o "out/p$1" "tests/erl/p$1/$2.erl"
  tools/do/build-do.sh "out/p$1" "out/do-p$1" "$3" "$2"
}
T=("$@")
[[ ${#T[@]} == 0 ]] && T=(p3 p4 p5)
for t in "${T[@]}"; do
  case $t in
  p3) build 3 pp afterglow-p3-pingpong; node tools/do/p3-test.mjs out/do-p3/wrangler.jsonc ;;
  p4) CRON="* * * * *" CRON_NODE=t CRON_PROC=ticker build 4 tm afterglow-p4-timers
      node tools/do/p4-test.mjs out/do-p4/wrangler.jsonc "${P4_MS:-5000}" ;;
  p5) build 5 chat afterglow-p5-chat; node tools/do/p5-test.mjs out/do-p5/wrangler.jsonc ;;
  *) echo "unknown test $t" >&2; exit 2 ;;
  esac
done
