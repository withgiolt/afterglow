#!/usr/bin/env bash
# package.sh [BIN_DIR]: fills gleam/priv for the Hex package, the files the giolt CLI
# builds with: afterglow_runtime.wasm, index.mjs and host.mjs (the glue that runs the app) and
# bin/afterglow-<platform>[.exe], one compiler per platform, named after Node's
# process.platform and process.arch.
# BIN_DIR holds those compilers (the release workflow downloads them there). Without
# it, only this machine's compiler is built, which is enough to test the CLI locally.
set -euo pipefail
cd "$(dirname "$0")/.."
priv=gleam/priv
rm -rf "$priv" && mkdir -p "$priv/bin"
cargo build -q --release -p afterglow_runtime --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/afterglow_runtime.wasm "$priv/"
cp tools/worker/index.mjs tools/worker/host.mjs "$priv/"
if [[ $# -ge 1 ]]; then
  cp "$1"/afterglow-* "$priv/bin/"
else
  cargo build -q --release -p afterglow
  platform=$(node -p 'process.platform + "-" + process.arch')
  if [[ -f target/release/afterglow.exe ]]; then
    cp target/release/afterglow.exe "$priv/bin/afterglow-$platform.exe"
  else
    cp target/release/afterglow "$priv/bin/afterglow-$platform"
  fi
fi
ls -l "$priv" "$priv/bin"
