#!/usr/bin/env bash
# difftest-project.sh PROJECT_DIR MODULE [METHOD] [URL] [BODY]
# HEADERS="name: value" lines (newline separated) add request headers; host is always sent.
# One request to MODULE:start/0 of a built Gleam project (`gleam build`), under real erl
# and under afterglow (Node), in the ETF shape host.mjs sends. Compares the decoded
# responses ({Status, Headers, Body}) byte for byte; stdout around the frame (erl's
# logger, io:format) is ignored. Reuses out/project/<module>.wasm if SKIP_BUILD=1.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
proj=$(cd "$1" && pwd) mod=$2 method=${3:-GET} url=${4:-/} body=${5:-}
cd "$here"
d=out/project; mkdir -p $d
tag=$mod$(echo "$method$url" | tr -c 'a-zA-Z0-9' '_')
req=$d/$tag.req
erl_body=${body//\\/\\\\};erl_body=${erl_body//\"/\\\"}
hdrs='{<<"host">>,<<"localhost">>}'
while IFS= read -r line; do
  [[ -n "$line" ]] && hdrs+=",{<<\"${line%%: *}\">>,<<\"${line#*: }\">>}"
done <<<"${HEADERS:-}"
erl -noshell -eval "file:write_file(\"$req\", term_to_binary({<<\"$method\">>,<<\"$url\">>,[$hdrs],<<\"$erl_body\">>})), halt()."
if [[ "${SKIP_BUILD:-}" != 1 || ! -f $d/$mod.wasm ]]; then
  args=(); for e in "$proj"/build/dev/erlang/*/ebin; do args+=(--beams "$e"); done
  tools/afterglow-build.sh $d/$mod.wasm --entry "$mod:start/0" "${args[@]}" 2>$d/$mod.build.log || { cat $d/$mod.build.log; exit 1; }
fi
set +e
node tools/run.mjs $d/$mod.wasm request <$req >$d/$tag.afterglow.out 2>$d/$tag.afterglow.err
rc=$?
set -e
pa=(); for e in "$proj"/build/dev/erlang/*/ebin; do pa+=(-pa "$e"); done
erl -noshell "${pa[@]}" -eval "{ok, _} = application:ensure_all_started(crypto), process_flag(trap_exit, false), $mod:start(), halt()." <$req >$d/$tag.erl.out 2>$d/$tag.erl.err || true
erl -noshell -eval "
  Dec = fun(F) ->
    {ok, B} = file:read_file(F),
    M = <<\"\\n__AFTERGLOW__\">>,
    case binary:matches(B, M) of
      [] -> none;
      Ms -> {P, _} = lists:last(Ms), Pre = binary:part(B, 0, P),
            <<_:(byte_size(Pre) - 4)/binary, L:32>> = Pre,
            binary_to_term(binary:part(Pre, byte_size(Pre) - 4 - L, L))
    end
  end,
  E = Dec(\"$d/$tag.erl.out\"), G = Dec(\"$d/$tag.afterglow.out\"),
  file:write_file(\"$d/$tag.erl.body\", case E of {_, _, Be} -> Be; _ -> <<>> end),
  file:write_file(\"$d/$tag.afterglow.body\", case G of {_, _, Bg} -> Bg; _ -> <<>> end),
  case E =:= G andalso E =/= none of
    true -> {S, _, Bd} = E, io:format(\"PASS ~s ~s ~s: status ~p, ~p bytes~n\", [\"$mod\", \"$method\", \"$url\", S, byte_size(Bd)]), halt(0);
    false -> io:format(\"FAIL ~s ~s ~s (node exit $rc)~n  erl:   ~P~n  afterglow: ~P~n\", [\"$mod\", \"$method\", \"$url\", E, 12, G, 12]), halt(1)
  end."
