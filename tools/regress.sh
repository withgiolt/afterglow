#!/usr/bin/env bash
# regress.sh: every difftest against erl (unit tests in tests/erl, all fixtures).
# Runs JOBS tests at a time (default: CPU count). Builds afterglow and afterglow_runtime once and skips
# wasm-opt (REGRESS_OPT=1 to test optimised builds). Logs: out/regress/<n>.log.
cd "$(dirname "$0")/.."
set -u
start=$(date +%s)
mkdir -p out out/regress && rm -f out/regress/*
erlc -o out tests/erl/*.erl 2>/dev/null
cargo build -q --release -p afterglow 2>/dev/null && cargo build -q --release -p afterglow_runtime --target wasm32-unknown-unknown 2>/dev/null || { echo "cargo build failed"; exit 1; }
export AFTERGLOW_PREBUILT=1
export AFTERGLOW_OTP_LIBS; AFTERGLOW_OTP_LIBS=$(tools/otp-libs.sh)
[[ "${REGRESS_OPT:-}" == 1 ]] || export AFTERGLOW_NOOPT=1

tests=()
add() { tests+=("$(printf '%q ' "$@")"); }
for f in m1:fib_test m1:sum_test m1:mixed_test \
         m2:t_match m2:t_split m2:t_bins m2:t_utf m2:t_build m2:t_ints m2:t_unicode m2:t_maps \
         m3:t_big m3:t_funs m3:t_exc m3:t_bigmap ur:t \
         p1e:t_basic p1e:t_links p1e:t_monitors p1e:t_timeouts p1e:t_selective p1e:t_misc p1t:t_timers \
         fmtx:t_directives fmtx:t_short \
         etfx:t_decode litfun:t numx:t_bases applyx:t_limit \
         cmpx:t_keys cmpx:t_zero; do
  add tools/difftest.sh "$f/0" --beams out
done
# The depth test measures the deployable build's stack limit: always optimised (unoptimised
# frames are bigger and overflow sooner).
add env AFTERGLOW_NOOPT= tools/difftest.sh m1_depth:d10k/0 --beams out
for f in m1_gleam:fib_test m1_gleam:sum_test; do
  add tools/difftest.sh "$f/0" --beams fixtures/app/build/dev/erlang/app/ebin --beams fixtures/app/build/dev/erlang/gleam_stdlib/ebin
done
eb=fixtures/app/build/dev/erlang; gb=(); for d in $eb/*/ebin; do gb+=(--beams "$d"); done
for f in p1:actor_test p1:pingpong_test; do add tools/difftest.sh "$f/0" "${gb[@]}"; done
for m in $(ls fixtures/app/src | sed -n 's/\.gleam$//p' | grep -v "fixture_check\|missing_function\|m1_gleam\|gc_churn\|^p1$"); do
  case $m in query_param) a=(GET "/?msg=hi");; not_found) a=(GET /anything);; echo_body) a=(POST / "hello body");; *) a=(GET /);; esac
  add tools/difftest-req.sh "$m" "${a[@]}"
done
for k in list bin "depth&n=5000"; do add tools/difftest-req.sh gc_churn GET "/?kind=$k"; done
add tools/p2-test.sh

jobs_max=${JOBS:-$(nproc 2>/dev/null || echo 4)}
run() {
  local n=$1; shift
  if eval "$*" >out/regress/$n.log 2>&1; then echo PASS >out/regress/$n.status
  else echo FAIL >out/regress/$n.status; echo "FAIL: $*"; head -5 out/regress/$n.log; fi
}
for i in "${!tests[@]}"; do
  while (( $(jobs -rp | wc -l) >= jobs_max )); do wait -n; done
  run "$i" "${tests[$i]}" &
done
wait
pass=$(grep -l PASS out/regress/*.status 2>/dev/null | wc -l)
fail=$(( ${#tests[@]} - pass ))
echo "regress: $pass passed, $fail failed ($(( $(date +%s) - start )) s, $jobs_max jobs)"
[[ $fail == 0 ]]
