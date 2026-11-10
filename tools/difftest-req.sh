#!/usr/bin/env bash
# difftest-req.sh MODULE [METHOD] [URL] [BODY]
# Builds fixture MODULE:start/0 with glowc, sends one request (the ETF shape glowvm's
# index.js sends) under Node and under erl, and compares stdout byte for byte.
# Files are per request (out/difftest-req/<tag>.*), so runs can go in parallel.
# GLOWC_OTP_EBIN: OTP's stdlib ebin dir, if the caller looked it up once.
set -euo pipefail
cd "$(dirname "$0")/.."
mod=$1 method=${2:-GET} url=${3:-/} body=${4:-}
eb=fixtures/app/build/dev/erlang
beams=()
for d in $eb/*/ebin; do beams+=(--beams "$d"); done
skip=()
for m in base64 io_lib_format json maps rand string uri_string; do skip+=(--skip "$m@/glowvm/"); done
d=out/difftest-req; mkdir -p $d
tag=$mod$(printf '%s' "_$method$url" | tr -c 'a-zA-Z0-9_' '_')
req=$d/$tag.req
erl -noshell -eval "file:write_file(\"$req\", term_to_binary({<<\"$method\">>,<<\"$url\">>,[{<<\"host\">>,<<\"localhost\">>}],<<\"$body\">>})), halt()."
if [[ "${SKIP_BUILD:-}" != 1 || ! -f $d/$tag.wasm ]]; then
  tools/glowc-build.sh $d/$tag.wasm --entry "$mod:start/0" "${beams[@]}" "${skip[@]}" 2>$d/$tag.build.log || { cat $d/$tag.build.log; exit 1; }
fi
set +e
node tools/run.mjs $d/$tag.wasm request <$req >$d/$tag.glowc.out 2>$d/$tag.glowc.err
rc=$?
set -e
otp=${GLOWC_OTP_EBIN:-$(erl -noshell -eval 'io:format("~s",[filename:dirname(code:which(lists))]),halt().')}
pa=(); for e in $eb/*/ebin; do pa+=(-pa "$e"); done
erl -noshell "${pa[@]}" -pa "$otp" -eval "{ok, _} = application:ensure_all_started(crypto), $mod:start(), halt()." <$req >$d/$tag.erl.out
if [[ $rc == 0 ]] && cmp -s $d/$tag.erl.out $d/$tag.glowc.out; then
  echo "PASS $mod $method $url ($(wc -c <$d/$tag.erl.out | tr -d ' ') bytes)"
else
  echo "FAIL $mod $method $url (node exit $rc)"; head -c 600 $d/$tag.glowc.err; echo
  cmp $d/$tag.erl.out $d/$tag.glowc.out || true
  exit 1
fi
