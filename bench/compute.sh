#!/usr/bin/env bash
# compute.sh: the workloads in bench/compute/src/compute.gleam, written once in Gleam, timed
# in-process on erl (Erlang/OTP), on afterglow (Node, V8) and on Gleam's JavaScript target
# (Node, V8). Each target runs WARMUP untimed iterations, then ITERS timed ones, and reports
# the median and p95. Nothing else is timed: no VM boot, Wasm instantiation or HTTP, which
# bench/vs-erl.sh measures. The stdlib and JSON implementations differ per target, and that
# difference is part of what's measured.
# Defaults: WARMUP=20, ITERS=100.
# Prints each workload's result line and a markdown table, and fails when erl and afterglow
# give different results for any workload. JavaScript results aren't compared: a JS Int is a
# float, so bignum shows n/a there.
set -euo pipefail
cd "$(dirname "$0")/.."
WARMUP=${WARMUP:-20} ITERS=${ITERS:-100}
d=out/compute; mkdir -p $d

(cd bench/compute && gleam build --target erlang >/dev/null && gleam build --target javascript >/dev/null)
cargo build -q --release -p afterglow
cargo build -q --release -p afterglow_runtime --target wasm32-unknown-unknown
export AFTERGLOW_PREBUILT=1 AFTERGLOW_OTP_LIBS="$(tools/otp-libs.sh)"
erlc -o $d bench/compute/bench_compute_erl.erl
beams=(); pa=(-pa $d)
for e in bench/compute/build/dev/erlang/*/ebin; do beams+=(--beams "$e"); pa+=(-pa "$e"); done
erl_eval() { erl -noshell "${pa[@]}" -eval "$1, halt()."; }

erl_eval 'bench_compute_erl:inputs()' >$d/inputs.tsv
js_module=bench/compute/build/dev/javascript/compute/compute.mjs
results=$d/results.jsonl; : >$results
while IFS=$'\t' read -r w _; do
  tools/afterglow-build.sh $d/$w.wasm --entry "compute:$w/0" "${beams[@]}" 2>$d/$w.build.log || { cat $d/$w.build.log; exit 1; }
  erl=$(erl_eval "bench_compute_erl:main([\"$w\", \"$WARMUP\", \"$ITERS\"])")
  ag=$(node bench/compute/harness.mjs afterglow $d/$w.wasm "$w" "$WARMUP" "$ITERS")
  js=null
  [[ $w == bignum ]] || js=$(node bench/compute/harness.mjs js $js_module "$w" "$WARMUP" "$ITERS")
  echo "{\"workload\":\"$w\",\"erl\":$erl,\"afterglow\":$ag,\"js\":$js}" >>$results
done <$d/inputs.tsv

cpu=$(node -p 'require("os").cpus()[0].model.trim()')
os=$(node -p 'const os = require("os"); `${os.type()} ${os.release()} ${os.arch()}`')
otp=$(erl -noshell -eval 'io:format("~s (~s)", [erlang:system_info(otp_release), erlang:system_info(emu_flavor)]), halt().')
version=$(sed -n 's/^version = "\(.*\)"/\1/p' gleam/gleam.toml)
env_line="$(date -u +%Y-%m-%d), $cpu, $os. OTP $otp, Node $(node --version), afterglow $version ($(git rev-parse --short HEAD))"
node -e '
  const fs = require("fs");
  const [results, inputs, warmup, iters, env] = process.argv.slice(1);
  const rows = fs.readFileSync(results, "utf8").trim().split("\n").map(JSON.parse);
  const input = new Map(fs.readFileSync(inputs, "utf8").trim().split("\n").map((l) => l.split("\t")));
  const time = (t) => (t ? `${t.median_ms.toFixed(3)} / ${t.p95_ms.toFixed(3)} ms` : "n/a");
  const out = [
    `${env}. ${warmup} warm-up and ${iters} timed iterations per workload and target, median / p95.`,
    "",
    "| Workload | Input | erl | afterglow | Gleam JS |",
    "| --- | --- | --- | --- | --- |",
    ...rows.map((r) => `| ${r.workload} | ${input.get(r.workload)} | ${time(r.erl)} | ${time(r.afterglow)} | ${time(r.js)} |`),
    "",
    "Results:",
    "",
    ...rows.map((r) => `- ${r.workload}: erl ${r.erl.result}, afterglow ${r.afterglow.result}, Gleam JS ${r.js ? r.js.result : "n/a"}`),
  ];
  console.log(out.join("\n"));
  const differ = rows.filter((r) => r.erl.result !== r.afterglow.result).map((r) => r.workload);
  if (differ.length) {
    console.error(`erl and afterglow give different results for: ${differ.join(", ")}`);
    process.exit(1);
  }
' $results $d/inputs.tsv "$WARMUP" "$ITERS" "$env_line"
