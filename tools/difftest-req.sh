#!/usr/bin/env bash
# difftest-req.sh MODULE [METHOD] [URL] [BODY]
# Builds fixture MODULE:start/0 with glowc, sends one request (the ETF shape glowvm's
# index.js sends) under Node and under erl, and compares stdout byte for byte.
set -euo pipefail
cd "$(dirname "$0")/.."
mod=$1 method=${2:-GET} url=${3:-/} body=${4:-}
eb=fixtures/app/build/dev/erlang
beams=()
for d in $eb/*/ebin; do beams+=(--beams "$d"); done
skip=()
for m in base64 io_lib_format json maps rand string uri_string; do skip+=(--skip "$m@/glowvm/"); done
d=out/difftest-req; mkdir -p $d
req=$d/$mod.req
erl -noshell -eval "file:write_file(\"$req\", term_to_binary({<<\"$method\">>,<<\"$url\">>,[{<<\"host\">>,<<\"localhost\">>}],<<\"$body\">>})), halt()."
if [[ "${SKIP_BUILD:-}" != 1 || ! -f $d/$mod.wasm ]]; then
  tools/glowc-build.sh $d/$mod.wasm --entry "$mod:start/0" "${beams[@]}" "${skip[@]}" 2>$d/$mod.build.log || { cat $d/$mod.build.log; exit 1; }
fi
set +e
node tools/run.mjs $d/$mod.wasm request <$req >$d/$mod.glowc.out 2>$d/$mod.glowc.err
rc=$?
set -e
otp=$(erl -noshell -eval 'io:format("~s",[filename:dirname(code:which(lists))]),halt().')
pa=(); for e in $eb/*/ebin; do pa+=(-pa "$e"); done
erl -noshell "${pa[@]}" -pa "$otp" -eval "{ok, _} = application:ensure_all_started(crypto), $mod:start(), halt()." <$req >$d/$mod.erl.out
if [[ $rc == 0 ]] && cmp -s $d/$mod.erl.out $d/$mod.glowc.out; then
  echo "PASS $mod ($(wc -c <$d/$mod.erl.out | tr -d ' ') bytes)"
else
  echo "FAIL $mod (node exit $rc)"; head -c 600 $d/$mod.glowc.err; echo
  cmp $d/$mod.erl.out $d/$mod.glowc.out || true
  exit 1
fi
