#!/usr/bin/env bash
# survey.sh: lists the opcodes and BIFs afterglow can't compile yet, over all fixtures.
cd "$(dirname "$0")/.."
otp=$(erl -noshell -eval 'io:put_chars(code:root_dir()), halt().')
args=(--rt target/wasm32-unknown-unknown/release/afterglow_runtime.wasm)
for d in fixtures/app/build/dev/erlang/*/ebin "$otp"/lib/stdlib-*/ebin "$otp"/lib/kernel-*/ebin "$otp"/lib/erts-*/ebin; do args+=(--beams "$d"); done
for m in "$@"; do
  target/release/afterglow build "${args[@]}" --entry "$m:start/0" -o /tmp/survey.wasm 2>&1 | sed -n "/failed to compile/,/first errors/p" | grep -v "failed to compile\|first errors" | sed "s/^/$m /"
done
