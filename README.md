# afterglow

afterglow runs Gleam web apps on Cloudflare Workers. It compiles an app's BEAM
bytecode to WebAssembly ahead of time and links it with a small runtime written
in Rust.

**Status: experimental research preview.** Apps built with afterglow can be
deployed on [Giolt](https://giolt.com), but afterglow is not for production use,
and breaking changes will come without notice. It runs a subset of OTP with
different process semantics from the BEAM, listed under
[Differences from Erlang](#differences-from-erlang). The Durable Object mode
described under [Design](#design) isn't part of the Hex package and can't be
deployed on Giolt.

afterglow supports Erlang/OTP 28 and 29. The tests also run on OTP master, where a
failure doesn't fail the run, so changes to the bytecode erlc emits show up before
they reach a release.

## Using it

### Through the giolt CLI

The [giolt CLI](https://github.com/withgiolt/cli) builds projects with
`target = "erlang"` with afterglow, runs them locally and deploys them to Giolt:

```sh
gleam run -m giolt build
```

[How to use afterglow](https://docs.giolt.com/afterglow/how-to-use) walks through
an app, and [About afterglow](https://docs.giolt.com/afterglow/about) lists what
doesn't work yet.

### Manually, without Giolt

The same build works by hand. It needs Gleam, Erlang/OTP 28 or 29, rebar3,
Node.js 22 or newer, and the `wasm-merge` and `wasm-opt` tools from
[Binaryen](https://github.com/WebAssembly/binaryen). The commands below run
Binaryen 132 from npm. The Hex package ships a prebuilt compiler for Linux and
macOS (x64 and arm64) and Windows (x64), so Rust isn't needed.

1. Add afterglow and wisp to an Erlang-target project:

   ```sh
   gleam new my_app && cd my_app
   # set target = "erlang" in gleam.toml
   gleam add afterglow wisp
   ```

2. Export `start/0` from a module, here `src/my_app.gleam`, and have it call
   `afterglow.serve` with a wisp handler:

   ```gleam
   import afterglow
   import wisp

   pub fn start() {
     afterglow.serve(handle)
   }

   fn handle(_req: wisp.Request) -> wisp.Response {
     wisp.ok() |> wisp.string_body("hello from afterglow")
   }
   ```

3. Compile to BEAM, then compile every function `start/0` reaches, including the
   OTP modules it calls, into one Wasm module. Use the compiler that matches
   your platform:

   ```sh
   gleam build
   priv=build/packages/afterglow/priv
   chmod +x $priv/bin/afterglow-linux-x64
   beams=()
   for d in build/dev/erlang/*/ebin; do beams+=(--beams "$d"); done
   for app in stdlib kernel erts; do
     beams+=(--beams "$(erl -noshell -eval "io:put_chars(code:lib_dir($app)), halt().")/ebin")
   done
   mkdir -p build/afterglow dist
   $priv/bin/afterglow-linux-x64 build --rt $priv/afterglow_runtime.wasm \
     --entry my_app:start/0 "${beams[@]}" -o build/afterglow/app.wasm
   ```

4. Link the module with the runtime, optimise it, and copy the Worker glue next
   to it:

   ```sh
   features="--enable-tail-call --enable-bulk-memory --enable-nontrapping-float-to-int
     --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue"
   npx -p binaryen@132.0.0 wasm-merge $features \
     $priv/afterglow_runtime.wasm afterglow_runtime build/afterglow/app.wasm app \
     -o build/afterglow/merged.wasm
   npx -p binaryen@132.0.0 wasm-opt $features -O3 build/afterglow/merged.wasm -o dist/app.wasm
   cp $priv/index.mjs $priv/host.mjs dist/
   ```

`dist/` is now a Worker. `index.mjs` is its entry, and `host.mjs` passes each
request into `app.wasm` and the response back out. Run it with wrangler from
`dist/`, with this `wrangler.jsonc`:

```jsonc
{
  "name": "my-app",
  "main": "index.mjs",
  "compatibility_date": "2025-08-06"
}
```

```sh
npx wrangler dev
```

String values in the Worker's env, such as `vars`, reach the app as its
environment. Bindings don't.

To run it on Node, load `app.wasm` yourself, because `index.mjs` imports it the
way Workers do:

```js
// run.mjs
import { readFileSync } from "node:fs";
import { instantiate } from "./dist/host.mjs";

const app = instantiate(new WebAssembly.Module(readFileSync("./dist/app.wasm")));
const response = await app.handle(new Request("http://localhost/"), {});
console.log(response.status, await response.text());
```

From a clone of this repository, `tools/build-project.sh ../my_app my_app dist`
does steps 3 and 4 with a compiler and runtime built from source, which needs
Rust with the `wasm32-unknown-unknown` target and Binaryen on `PATH`.

## Design

### The pipeline

1. `crates/beam-reader` parses each `.beam` file: its chunks, its atom, import,
   export, fun and literal tables, and its `Code` chunk, decoded into a flat list
   of instructions per function.
2. `crates/afterglow`, the compiler, walks reachability from the entry function
   across the app, its dependencies and the `stdlib`, `kernel` and `erts`
   modules of the OTP that `erl` runs on the build machine. It translates each
   reachable function's BEAM instructions to Wasm and numbers atoms, literals,
   functions and funs across the whole program. Nothing interprets bytecode at
   run time.
3. A call goes to one of three places: a function compiled from a `.beam` file,
   a built-in function the runtime exports under its Erlang name (such as
   `erlang:length/1`), or, when neither exists, a stub that raises `undef`. The
   compiler prints the missing ones, and the build still succeeds.
4. `wasm-merge` links the result with `crates/afterglow_runtime`, compiled to
   `wasm32-unknown-unknown`, into one module, and `wasm-opt -O3` optimises it.

### What each app's Wasm contains

Everything the app runs is inside its `app.wasm`: the app, its dependencies, the
OTP code it reaches, and the runtime with its built-in functions, memory
manager and scheduler. The host, `tools/worker/host.mjs`, provides three
imports (a log line, random bytes and the wall clock) and converts requests and
responses to and from the Erlang term format.

Because OTP ships inside each app, only the Erlang parts of OTP come along. The
parts the BEAM implements in C, such as BIFs and the NIFs behind `crypto`, `re`
and `ets`, have to be rewritten in Rust in the runtime one by one, and services
that live in their own processes on a running node, such as the code server,
the application controller or the logger, don't exist. Every OTP module an app
reaches also adds to its Wasm, which counts towards the Worker size limit and
its startup time. So afterglow runs the subset of OTP a request handler needs,
not OTP as a whole.

### Processes

afterglow implements `spawn`, `send`, selective `receive`, links, monitors, exit
signals, registered names and timers, but it schedules processes cooperatively,
on the single thread a Worker has:

- A process runs until it waits in a `receive`, exits or finishes. There are no
  reductions and no preemption, so a process that loops without receiving
  blocks every other one.
- There is no process isolation. Every process shares one heap and one Wasm
  instance, and sending a message doesn't copy it. An exception exits only its
  process, as on the BEAM, but a trap, such as running out of memory, ends the
  whole request.
- Time is virtual in Worker mode. A `receive` timeout or a timer fires only when
  no process can run. That gives erl's result for any program whose outcome
  doesn't depend on wall-clock races.

### Worker mode

This is the mode Giolt deploys. One Wasm instance serves many requests in turn.
Each request starts a main process that runs the handler, and when the handler
returns, the request ends. Processes it spawned end with it, and the next
request starts from an empty heap. Nothing a request leaves in memory reaches
the next one.

### Durable Object mode (experimental)

In this mode one Durable Object hosts one long-lived afterglow node. Processes,
the heap and timers persist across requests, which this mode calls turns. After
each turn, the host writes every process that changed to the Durable Object's
SQLite storage as Erlang terms, and it restores them when the object wakes up.
Timers use the Durable Object alarm, the host delivers messages to processes in
other Durable Objects after the turn, and WebSockets use the hibernation API. A
redeploy restarts `gen_server` processes from their last state through
`code_change`. Scheduling is still cooperative and processes are still not
isolated.

Only this repository can build the mode. `tools/do/test.sh` builds the Erlang
test nodes in `tests/erl/p3` to `p5` (ping-pong, timers, chat) and runs them on
workerd, and `tools/do/deployed.sh` checks them on a Cloudflare account.
The Hex package doesn't include it, and Giolt and the giolt CLI can't deploy it.

### Memory and garbage collection

Processes allocate on one heap by bumping a pointer through chained blocks.
Allocation never collects. When a block fills, the allocator asks for a
collection, which runs at the next safe point, a function entry or a heap check,
where the compiled code knows exactly which registers and stack slots are live.
The collector, a Cheney copying collector, moves live terms into fresh blocks.
Binary bytes live in a separate arena, outside the heap, which the collector
compacts once it has grown enough. The runtime decodes literals once per
instance into a permanent area. In Worker mode it resets the heap and the arena
at the start of each request, and it zeroes freed blocks so a later request
can't read an earlier one's data.

The main process has a 128 KiB stack and each spawned process has 64 KiB, so
deep non-tail recursion overflows them sooner than on the BEAM.

## Differences from Erlang

What matches, because the input is the same BEAM bytecode:

- Integers have arbitrary precision.
- Binaries and bitstrings are built and matched as on the BEAM.
- `gleam_stdlib` gives the same output as on Erlang. `tools/regress.sh` builds
  each fixture in `fixtures/app` and the Erlang tests in `tests/erl`, runs them
  on afterglow and on `erl`, and fails on any difference.

What doesn't:

- **No preemption.** A process runs until it waits, as [Processes](#processes)
  describes.
- **No process isolation.** Processes share one heap and one instance.
- **Process lifetime depends on the mode.** In Worker mode, processes end with
  the request. Only the Durable Object mode keeps them, and Giolt can't deploy
  it.
- **A subset of OTP.** Functions afterglow doesn't provide raise `undef`, and the
  request gets a 500 whose body names the function. The build lists them.
- **No ETS and no `re`**, so `gleam_regexp` and libraries that use ETS tables
  fail.
- **No crypto hashing or signing yet.** Only `crypto:strong_rand_bytes/1` works.
  `crypto:hash*` and `crypto:mac/4` don't, so `gleam_crypto` hashing, HMAC, and
  wisp's signed cookies and sessions don't work.
- **No OS access.** Apps have no file system, ports, `os:cmd/1`, distributed
  Erlang or hot code loading.
- **No outbound HTTP** from Erlang code yet.
- **No file responses.** A `wisp.File` body returns 501.
- **`erlang:phash2`** returns different values, and small maps with atom keys can
  iterate in a different order.
- **Deep recursion.** Non-tail recursion deeper than about 12,000 calls in the
  main process overflows the stack, and spawned processes have half its size.
  That request gets a 502, and the next one works.
- **Introspection.** `process_info/2` and `erlang:statistics/1` report 0 for
  reductions, context switches and heap sizes.

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
