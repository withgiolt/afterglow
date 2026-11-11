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

## Layout

- `crates/afterglow`: the compiler, from BEAM files to a Wasm module
- `crates/afterglow_runtime`: the runtime the compiled app links with
- `crates/beam-reader`: the BEAM file parser the compiler uses
- `gleam/`: the `afterglow` Hex package, with `afterglow.serve` and, once
  `tools/package.sh` fills `priv/`, the compilers and the runtime
- `tools/worker`: `host.js`, the library a Worker entry runs an app with (the Hex
  package ships it; the platform deploying the app writes the entry), and `index.js`,
  the Worker entry the tests use

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
compiler for each platform and publishes the Hex package.

## Issues

Report bugs and requests at [withgiolt/issues](https://github.com/withgiolt/issues/issues).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
