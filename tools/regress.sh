#!/usr/bin/env bash
# regress.sh: every difftest against erl (unit tests in tests/erl, all fixtures).
cd "$(dirname "$0")/.."
mkdir -p out && erlc -o out tests/erl/*.erl 2>/dev/null
pass=0 fail=0
t() { if "$@" >/tmp/regress.out 2>&1; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL: $*"; head -5 /tmp/regress.out; fi; }
for f in m1:fib_test m1:sum_test m1:mixed_test m1_depth:d10k \
         m2:t_match m2:t_split m2:t_bins m2:t_utf m2:t_build m2:t_ints m2:t_unicode m2:t_maps \
         m3:t_big m3:t_funs m3:t_exc m3:t_bigmap ur:t \
         p1e:t_basic p1e:t_links p1e:t_monitors p1e:t_timeouts p1e:t_selective p1e:t_misc p1t:t_timers; do
  t tools/difftest.sh "$f/0" --beams out
done
for f in m1_gleam:fib_test m1_gleam:sum_test; do
  t tools/difftest.sh "$f/0" --beams fixtures/app/build/dev/erlang/app/ebin --beams fixtures/app/build/dev/erlang/gleam_stdlib/ebin
done
eb=fixtures/app/build/dev/erlang; gb=(); for d in $eb/*/ebin; do gb+=(--beams "$d"); done
for m in base64 io_lib_format json maps rand string uri_string; do gb+=(--skip "$m@/glowvm/"); done
for f in p1:actor_test p1:pingpong_test; do t tools/difftest.sh "$f/0" "${gb[@]}"; done
for m in $(ls fixtures/app/src | sed -n 's/\.gleam$//p' | grep -v "build_all\|fixture_check\|m1_gleam\|gc_churn\|^p1$"); do
  case $m in query_param) a=(GET "/?msg=hi");; not_found) a=(GET /anything);; echo_body) a=(POST / "hello body");; *) a=(GET /);; esac
  t tools/difftest-req.sh "$m" "${a[@]}"
done
for k in list bin "depth&n=5000"; do t tools/difftest-req.sh gc_churn GET "/?kind=$k"; done
t tools/p2-test.sh
echo "regress: $pass passed, $fail failed"
[[ $fail == 0 ]]
