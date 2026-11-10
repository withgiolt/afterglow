# afterglow

Runs Gleam web apps on Cloudflare Workers. afterglow compiles your app's BEAM
bytecode to WebAssembly ahead of time and links it with a small Rust runtime.

You don't add this package yourself: `giolt build` uses it for projects with
`target = "erlang"`. Your entry module exports `handle`:

```gleam
pub fn handle(req: wisp.Request) -> wisp.Response
```

The package holds `afterglow.serve`, which the generated start module calls,
plus the compiler for each platform and the runtime in `priv/`.

Source and issues: [github.com/withgiolt/afterglow](https://github.com/withgiolt/afterglow).
