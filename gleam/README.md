# afterglow

Runs Gleam web apps on Cloudflare Workers. afterglow compiles your app's BEAM
bytecode to WebAssembly ahead of time and links it with a small Rust runtime.

afterglow is an experimental research preview: apps built with it can be
deployed on [Giolt](https://giolt.com), but it is not for production use, and
breaking changes will come without notice.

The [giolt CLI](https://github.com/withgiolt/cli) builds projects with
`target = "erlang"` with this package, or you can add it yourself and build
without Giolt. [Using it](https://github.com/withgiolt/afterglow#using-it) in the
repository's README covers both.

The package holds `afterglow.serve`, which runs a wisp handler from your
`start/0`:

```gleam
pub fn start() {
  afterglow.serve(handle)
}
```

Its `priv/` directory holds the runtime, a compiler for Linux and macOS (x64 and
arm64) and Windows (x64), and the Worker glue that goes next to `app.wasm`:
`index.mjs` and the `host.mjs` it imports.

Some OTP functions aren't implemented yet. The build lists the ones your app
reaches, and a request that calls one gets a 500 naming it.
[Differences from Erlang](https://github.com/withgiolt/afterglow#differences-from-erlang)
lists the current limits.

- Source: [github.com/withgiolt/afterglow](https://github.com/withgiolt/afterglow)
- Issues: [github.com/withgiolt/issues](https://github.com/withgiolt/issues/issues)
- License: GPL-3.0-or-later
