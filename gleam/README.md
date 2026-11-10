# afterglow

Runs Gleam web apps on Cloudflare Workers. afterglow compiles your app's BEAM
bytecode to WebAssembly ahead of time and links it with a small Rust runtime.

afterglow is experimental, so expect breaking changes.

You don't add this package yourself. `giolt build` uses it for projects with
`target = "erlang"`, and needs Gleam and Erlang/OTP installed. Your entry module
exports `handle`:

```gleam
pub fn handle(req: wisp.Request) -> wisp.Response
```

The package holds `afterglow.serve`, which the generated start module calls.
Its `priv/` directory holds the runtime and a compiler for Linux and macOS (x64
and arm64) and Windows (x64).

Some OTP functions aren't implemented yet. `giolt build` lists the ones your app
reaches, and a request that calls one gets a 500 naming it. The
[docs](https://docs.giolt.com/afterglow/about) list the current limits.

- Source: [github.com/withgiolt/afterglow](https://github.com/withgiolt/afterglow)
- Issues: [github.com/withgiolt/issues](https://github.com/withgiolt/issues/issues)
- License: GPL-3.0-or-later
