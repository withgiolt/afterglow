# afterglow

afterglow runs Gleam web apps on Cloudflare Workers. It compiles an app's BEAM
bytecode to WebAssembly ahead of time and links it with a small runtime written
in Rust. It is experimental, so expect breaking changes.

## Using it

You don't install afterglow yourself. The [giolt CLI](https://github.com/withgiolt/cli)
builds projects with `target = "erlang"` with it:

```sh
gleam run --target javascript -m giolt build
```

[How to use afterglow](https://docs.giolt.com/afterglow/how-to-use) walks through
an app, and [About afterglow](https://docs.giolt.com/afterglow/about) lists what
doesn't work yet.

afterglow supports Erlang/OTP 28 and 29. The tests also run on OTP master, where a
failure doesn't fail the run, so changes to the bytecode erlc emits show up before
they reach a release.

## Layout

- `crates/afterglow`: the compiler, from BEAM files to a Wasm module
- `crates/afterglow_runtime`: the runtime the compiled app links with
- `crates/beam-reader`: the BEAM file parser the compiler uses
- `gleam/`: the `afterglow` Hex package, with `afterglow.serve` and, once
  `tools/package.sh` fills `priv/`, the compilers and the runtime
- `tools/worker`: the glue the Hex package ships next to an app's `app.wasm`:
  `index.mjs`, the Worker entry, and `host.mjs`, which passes each request and the
  vars the caller hands it into the Wasm

## Tests

The tests need Erlang/OTP, rebar3, Rust (with the `wasm32-unknown-unknown`
target), Binaryen, Bun and Node 22 or newer. Build the fixture app with
`gleam build` in `fixtures/app`, then run:

- `tools/regress.sh`, which compares afterglow's output with real `erl`
- `tools/smoke/run.sh`, which runs the fixture Workers on workerd
- `bun test tests/smoke.test.ts tests/host.test.ts`, after `tools/smoke/build.sh`
- `tools/do/test.sh`, which runs the Durable Object Workers on workerd

`just test-all` runs all of them.

On Windows, run them from Git Bash after `source tools/env.sh`.

## Releases

Pushing a `v*` tag that matches the version in `gleam/gleam.toml` builds the
compiler for each platform and publishes the Hex package. The tagged commit
must be on `main`, or the publish fails.

## AI use

AI may write code in every Giolt repository, and this one is heavily AI-assisted: agents critique its structure, proofread, and write the tedious parts, such as the BEAM file parser.

How AI use is managed here:

- Every change reaches `main` through a pull request with passing CI, and CI publishes releases only from `main`. GitHub enforces both, for everyone.
- Before each commit and merge, an agent waits for the maintainer to confirm they reviewed the change, and the commit records it in a `Reviewed-by:` trailer. GitHub doesn't check this. It is a rule the agents follow.
- A change to core code, where a quiet mistake could leak a token, lose data, ship a broken deploy or break every build, gets a code review pass first, and the agent names each core file it touched.

The agents' rules are in [AI.md](AI.md). The [AI use policy](https://giolt.com/ai) covers every Giolt repository.

## Issues

Report bugs and requests at [withgiolt/issues](https://github.com/withgiolt/issues/issues).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
