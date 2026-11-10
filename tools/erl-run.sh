#!/usr/bin/env bash
# Run a fixture's start/0 under real erl with one request on stdin, the same ETF
# shape index.js sends. Prints the decoded {Status, Headers, Body} response.
# Usage: tools/erl-run.sh <module> [method] [url]
set -euo pipefail
cd "$(dirname "$0")/.."
mod=$1 method=${2:-GET} url=${3:-/}
eb=fixtures/app/build/dev/erlang
pa=$(for d in $eb/*/ebin; do echo -n " -pa $d"; done)
req=$(erl -noshell -eval "io:format(\"~s\",[base64:encode(term_to_binary({<<\"$method\">>,<<\"$url\">>,[{<<\"host\">>,<<\"localhost\">>}],<<>>}))]),halt().")
echo -n "$req" | base64 -d > /tmp/afterglow-req.etf
erl -noshell $pa -eval "
  {ok, _} = application:ensure_all_started(crypto),
  $mod:start(), halt()." < /tmp/afterglow-req.etf > /tmp/afterglow-out.bin
erl -noshell -eval '
  {ok, B} = file:read_file("/tmp/afterglow-out.bin"),
  S = byte_size(<<"\n__AFTERGLOW__">>),
  Pre = binary:part(B, 0, byte_size(B) - S),
  <<_:(byte_size(Pre) - 4)/binary, Len:32>> = Pre,
  T = binary_to_term(binary:part(Pre, byte_size(Pre) - 4 - Len, Len)),
  {St, H, Body} = T,
  io:format("status ~p~nheaders ~p~nbody ~p bytes: ~s~n", [St, H, byte_size(Body), binary:part(Body, 0, min(200, byte_size(Body)))]),
  halt().'
