#!/usr/bin/env bash
# p2-test.sh: P2 checkpoint/restore under the Node DO harness. A counter gen_server
# (v1: integer state) survives eviction, then a redeploy to v2 (map state) through
# code_change, then another eviction. Expected replies are what erl's semantics give.
set -euo pipefail
cd "$(dirname "$0")/.."
for v in 1 2; do
  mkdir -p out/p2v$v
  erlc -o out/p2v$v tests/erl/p2/afterglow_node.erl tests/erl/p2v$v/counter.erl
  tools/afterglow-build.sh out/p2v$v.wasm --beams out/p2v$v --keep counter --keep gen --keep gen_server --keep afterglow_node --entry afterglow_node:turn/0 2>/dev/null
done
got=$(node tools/node-harness.mjs tests/p2-scenario.json | node -e 'const l=JSON.parse(require("fs").readFileSync(0));console.log(l.filter(s=>s.call).map(s=>s.reply).join(" "))')
want="{v1,1} {v1,2} {v1,3} {v2,4} {v2,4} {v2,5}"
if [[ "$got" == "$want" ]]; then echo "PASS p2 checkpoint/restore/upgrade: $got"; else echo "FAIL p2: got $got, want $want"; exit 1; fi
