# AGENTS.md

afterglow compiles BEAM bytecode from Gleam and Erlang ahead of time to WebAssembly, linked with a Rust runtime (`crates/afterglow_runtime`), so Giolt can run BEAM apps on Cloudflare Workers and Durable Objects. Apps call `afterglow.serve` from the Gleam package in `gleam/`.

On Windows, run the tools from Git Bash after `source tools/env.sh` (Erlang and Binaryen on PATH). Without Developer Mode, the first `gleam build` of a project must run elevated through `tools/gleam-elevated.ps1 "<dir>|build"`; later builds don't. Tests need the fixture app built: `gleam build` in `fixtures/app`. Then `tools/regress.sh` (difftests against erl), `tools/smoke/run.sh` (the Workers on workerd), `bun test tests/smoke.test.ts tests/host.test.ts` (after `tools/smoke/build.sh`) and `tools/do/test.sh` (the Durable Object Workers on workerd). `tools/difftest-project.sh DIR MODULE METHOD PATH` compares any built Gleam project with erl. Pass URL paths to `bun tools/fetch-worker.ts` with `MSYS_NO_PATHCONV=1`.

Related repos: `../giolt` (the platform that uploads Workers), `../cli` (the giolt CLI, which gets the runtime switch) and `../docs` (the docs site).

Coding agents follow [AI.md](AI.md): review before each commit and merge, the settings, and how to work here. Read it before changing anything.

@AI.md
