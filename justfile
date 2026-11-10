# Recipes run in Git Bash on Windows, not the WSL bash.exe that comes first on PATH.
# BASH_ENV puts Erlang and Binaryen on PATH there (a no-op elsewhere).
set shell := ["bash", "-cu"]
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-cu"]

export BASH_ENV := "tools/env.sh"

default:
    @just --list

# The compiler and the runtime, release builds.
build:
    cargo build --release -p afterglow
    cargo build --release -p afterglow_runtime --target wasm32-unknown-unknown

# Build the fixture app (on Windows without Developer Mode, first build it via tools/gleam-elevated.ps1).
build-fixtures:
    cd fixtures/app && gleam build

# Every difftest against erl. Needs `just build-fixtures`.
test:
    tools/regress.sh

# The fixture Workers on workerd.
test-workers *fixtures:
    tools/smoke/run.sh {{ fixtures }}

# Build the fixture Workers, then run tests/smoke.test.ts.
test-smoke *fixtures:
    tools/smoke/build.sh {{ fixtures }}
    bun test tests/smoke.test.ts

# The P3-P5 Durable Object Workers on local workerd, e.g. just test-do p4.
test-do *tests:
    tools/do/test.sh {{ tests }}

# Everything: difftests, workerd, smoke, Durable Objects.
test-all: build-fixtures test test-workers test-smoke test-do

# Compare a built Gleam project with erl, e.g. just difftest-project DIR app GET /
difftest-project dir module method="GET" path="/" *body:
    MSYS_NO_PATHCONV=1 tools/difftest-project.sh {{ dir }} {{ module }} {{ method }} {{ path }} {{ body }}

# One request through a built Worker: `just fetch-worker out/workers/basic_get /`.
fetch-worker dir *args:
    MSYS_NO_PATHCONV=1 bun tools/fetch-worker.ts {{ dir }} {{ args }}

# Opcodes and BIFs afterglow can't compile yet, for the given fixture modules.
survey *modules:
    tools/survey.sh {{ modules }}

# Per-function check of the BEAM reader against beam_disasm.
reader-check:
    tools/reader-check.sh

# Fill gleam/priv for the Hex package from the compilers in BIN_DIR (default: this machine only).
package *bin_dir:
    tools/package.sh {{ bin_dir }}

# Publish the Gleam package. Run `just package bin` with every platform's compiler first.
publish-hex:
    cd gleam && gleam build
    cd gleam && printf 'I am not using semantic versioning\n' | gleam publish --yes
