#!/usr/bin/env bash
# difftest.sh M:F/0 [afterglow options, e.g. --beams DIR]
# Compiles M:F/0 with afterglow, runs it under Node, runs it under erl, and compares the
# results with =:= (exceptions compared by class and reason).
set -euo pipefail
cd "$(dirname "$0")/.."
mfa=$1; shift
m=${mfa%%:*}; rest=${mfa#*:}; f=${rest%/*}
mkdir -p out/difftest
w=out/difftest/$m-$f.wasm
tools/afterglow-build.sh "$w" --entry "$mfa" "$@" 2>out/difftest/$m-$f.build.log || { cat out/difftest/$m-$f.build.log; exit 1; }
set +e
node tools/run.mjs "$w" test >out/difftest/$m-$f.etf 2>out/difftest/$m-$f.err
rc=$?
set -e
pa=()
for a in "$@"; do [[ -d "$a" ]] && pa+=(-pa "$a"); done
erl -noshell "${pa[@]}" -eval "
  Expected = try {ok, $m:$f()} catch C:R -> {exc, C, R} end,
  Got = case $rc of
          0 -> {ok, File} = file:read_file(\"out/difftest/$m-$f.etf\"), {ok, binary_to_term(File)};
          _ -> {ok, E} = file:read_file(\"out/difftest/$m-$f.err\"), {afterglow_failed, $rc, E}
        end,
  case Got =:= Expected of
    true -> io:format(\"PASS ~s~n\", [\"$mfa\"]), halt(0);
    false -> io:format(\"FAIL ~s~n  erl:   ~P~n  afterglow: ~P~n\", [\"$mfa\", Expected, 30, Got, 30]), halt(1)
  end."
