#!/usr/bin/env bash
# vs-erl.sh [FIXTURE...] : the same fixture app on vanilla erl (Erlang/OTP, JIT) and on
# afterglow (Node, V8), on this machine. Checks both return byte-identical responses,
# then measures per runtime:
#   cold      wall time from process start to exit after one request (VM or Node boot included)
#   init      afterglow only: instantiating app.wasm and init(), the part a running Worker pays
#   warm      mean time per request over N requests on one warm VM or instance
#   memory    erlang:memory(total) vs the Wasm linear memory, after the warm run
# Defaults: basic_get lustre_html json_roundtrip, RUNS=3 (median reported), N=20000
# (N/20 for json_roundtrip). Needs `gleam build` in fixtures/app first.
# Output: a markdown table on stdout and in bench/results/vs-erl.md.
set -euo pipefail
cd "$(dirname "$0")/.."
F=("$@"); [[ ${#F[@]} -gt 0 ]] || F=(basic_get lustre_html json_roundtrip)
RUNS=${RUNS:-3} N=${N:-20000}
d=out/vs-erl; mkdir -p $d
[[ -d fixtures/app/build/dev/erlang/afterglow/ebin ]] || { echo "build the fixture app first: cd fixtures/app && gleam build" >&2; exit 1; }

cargo build -q --release -p afterglow
cargo build -q --release -p afterglow_runtime --target wasm32-unknown-unknown
export AFTERGLOW_PREBUILT=1 AFTERGLOW_OTP_LIBS="$(tools/otp-libs.sh)"
erlc -o $d bench/vs-erl/bench_erl.erl bench/vs-erl/afterglow_io.erl
erl -noshell -eval "file:write_file(\"$d/get.req\", term_to_binary({<<\"GET\">>,<<\"/\">>,[{<<\"host\">>,<<\"localhost\">>}],<<>>})), halt()."
beams=(); pa=()
for e in fixtures/app/build/dev/erlang/*/ebin; do beams+=(--beams "$e"); pa+=(-pa "$e"); done

ms_since() { echo $(( ($(date +%s%N) - $1) / 1000000 )); }
results=$d/results.jsonl; : >$results
for f in "${F[@]}"; do
  n=$N; [[ $f == json_roundtrip ]] && n=$((N / 20))
  tools/afterglow-build.sh $d/$f.wasm --entry "$f:start/0" "${beams[@]}" 2>$d/$f.build.log || { cat $d/$f.build.log; exit 1; }
  for ((r = 1; r <= RUNS; r++)); do
    s=$(date +%s%N); node bench/vs-erl/afterglow.mjs $d/$f.wasm $d/get.req $d/$f.afterglow.out 1 >/dev/null; ag_cold=$(ms_since $s)
    s=$(date +%s%N); erl -noshell "${pa[@]}" -pa $d -eval "bench_erl:main([\"$f\", \"$d/get.req\", \"$d/$f.erl.out\", \"1\"])." >/dev/null; erl_cold=$(ms_since $s)
    cmp -s $d/$f.afterglow.out $d/$f.erl.out || { echo "$f: afterglow and erl responses differ ($d/$f.*.out)" >&2; exit 1; }
    ag=$(node bench/vs-erl/afterglow.mjs $d/$f.wasm $d/get.req $d/$f.afterglow.out $n)
    erl=$(erl -noshell "${pa[@]}" -pa $d -eval "bench_erl:main([\"$f\", \"$d/get.req\", \"$d/$f.erl.out\", \"$n\"]).")
    echo "{\"fixture\":\"$f\",\"n\":$n,\"afterglow\":$ag,\"erl\":$erl,\"afterglow_cold_ms\":$ag_cold,\"erl_cold_ms\":$erl_cold}" >>$results
  done
done
s=$(date +%s%N); erl -noshell -eval 'halt().'; erl_boot=$(ms_since $s)
s=$(date +%s%N); node -e 0; node_boot=$(ms_since $s)

env_line="$(uname -sm), $(node -p 'require("os").cpus()[0].model.trim()'), $(erl -noshell -eval 'io:format("OTP ~s (~s)", [erlang:system_info(otp_release), erlang:system_info(emu_flavor)]), halt().'), Node $(node --version), afterglow $(git rev-parse --short HEAD)"
node -e '
  const fs = require("fs");
  const [file, runs, erlBoot, nodeBoot, env] = process.argv.slice(1);
  const rows = fs.readFileSync(file, "utf8").trim().split("\n").map(JSON.parse);
  const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s[s.length >> 1]; };
  const time = (us) => (us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us.toFixed(1)} µs`);
  const mib = (kib) => `${(kib / 1024).toFixed(1)} MiB`;
  const out = [
    `Same app on both runtimes, responses byte-identical. Median of ${runs} runs. ${env}.`,
    `Bare boot, no app: erl ${erlBoot} ms, node ${nodeBoot} ms.`,
    "",
    "| Fixture | Warm per request: afterglow / erl | Cold, process start to response: afterglow / erl | afterglow init | Memory: afterglow / erl |",
    "| --- | --- | --- | --- | --- |",
  ];
  for (const f of [...new Set(rows.map((r) => r.fixture))]) {
    const rs = rows.filter((r) => r.fixture === f), m = (g) => median(rs.map(g));
    out.push(`| ${f} (n=${rs[0].n}) | ${time(m((r) => r.afterglow.per_request_us))} / ${time(m((r) => r.erl.per_request_us))}` +
      ` | ${m((r) => r.afterglow_cold_ms)} ms / ${m((r) => r.erl_cold_ms)} ms | ${m((r) => r.afterglow.init_ms).toFixed(1)} ms` +
      ` | ${mib(m((r) => r.afterglow.memory_kib))} / ${mib(m((r) => r.erl.memory_kib))} |`);
  }
  console.log(out.join("\n"));
' $results "$RUNS" "$erl_boot" "$node_boot" "$env_line" | tee bench/results/vs-erl.md
