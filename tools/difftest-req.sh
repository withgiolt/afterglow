#!/usr/bin/env bash
# difftest-req.sh MODULE [METHOD] [URL] [BODY]
# Builds fixture MODULE:start/0 with afterglow, sends one request (the ETF shape
# index.js sends) under Node and under erl, and compares stdout byte for byte.
# Files are per request (out/difftest-req/<tag>.*), so runs can go in parallel.
set -euo pipefail
cd "$(dirname "$0")/.."
mod=$1 method=${2:-GET} url=${3:-/} body=${4:-}
eb=fixtures/app/build/dev/erlang
beams=()
for d in $eb/*/ebin; do beams+=(--beams "$d"); done
d=out/difftest-req; mkdir -p $d
tag=$mod$(printf '%s' "_$method$url" | tr -c 'a-zA-Z0-9_' '_')
req=$d/$tag.req
erl -noshell -eval "file:write_file(\"$req\", term_to_binary({<<\"$method\">>,<<\"$url\">>,[{<<\"host\">>,<<\"localhost\">>}],<<\"$body\">>})), halt()."
if [[ "${SKIP_BUILD:-}" != 1 || ! -f $d/$tag.wasm ]]; then
  tools/afterglow-build.sh $d/$tag.wasm --entry "$mod:start/0" "${beams[@]}" 2>$d/$tag.build.log || { cat $d/$tag.build.log; exit 1; }
fi
set +e
node tools/run.mjs $d/$tag.wasm request <$req >$d/$tag.afterglow.out 2>$d/$tag.afterglow.err
rc=$?
set -e
pa=(); for e in $eb/*/ebin; do pa+=(-pa "$e"); done
erl -noshell "${pa[@]}" -eval "{ok, _} = application:ensure_all_started(crypto), $mod:start(), halt()." <$req >$d/$tag.erl.out
if [[ $rc == 0 ]] && cmp -s $d/$tag.erl.out $d/$tag.afterglow.out; then
  echo "PASS $mod $method $url ($(wc -c <$d/$tag.erl.out | tr -d ' ') bytes)"
else
  echo "FAIL $mod $method $url (node exit $rc)"; head -c 600 $d/$tag.afterglow.err; echo
  cmp $d/$tag.erl.out $d/$tag.afterglow.out || true
  exit 1
fi
