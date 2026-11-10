#!/usr/bin/env bash
# Golden test for beam-reader: per function, the count of instructions (excluding
# label/line) must match beam_disasm on every module the fixtures reach.
set -euo pipefail
cd "$(dirname "$0")/.."
OTP=$(erl -noshell -eval 'io:format("~s",[code:root_dir()]),halt().')
# An array, not a string: on Windows the OTP root has spaces ("C:/Program Files/Erlang OTP").
# nullglob: an OTP without crypto just skips it.
shopt -s nullglob
beams=("$OTP"/lib/stdlib-*/ebin/*.beam "$OTP"/lib/erts-*/ebin/erlang.beam "$OTP"/lib/kernel-*/ebin/{file,os,code}.beam \
  "$OTP"/lib/crypto-*/ebin/crypto.beam fixtures/app/build/dev/erlang/*/ebin/*.beam)
cargo build -q --release -p afterglow
target/release/afterglow dump "${beams[@]}" | sort > /tmp/rust-dump.txt
BEAMS=$(printf '%s\n' "${beams[@]}") erl -noshell -eval '
[begin
  {beam_file, M, _, _, _, Code} = beam_disasm:file(B),
  [io:format("~s:~s/~p ~p~n", [M, F, A, length([I || I <- Is, not is_tuple(I) orelse not lists:member(element(1,I), [label, line, executable_line])])]) || {function, F, A, _, Is} <- Code]
 end || B <- string:lexemes(os:getenv("BEAMS"), "\n")], halt().' | sort > /tmp/erl-dump.txt
echo "modules: ${#beams[@]}, functions: $(wc -l < /tmp/rust-dump.txt) (rust) $(wc -l < /tmp/erl-dump.txt) (erl)"
diff /tmp/rust-dump.txt /tmp/erl-dump.txt && echo "reader matches beam_disasm"
