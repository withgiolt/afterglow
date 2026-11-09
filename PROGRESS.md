# Phase 0 progress log

Working notes for the Phase 0 checks in `SPIKE.md` (the design doc, verbatim).
Update this file after every step so nothing is lost on context compaction.

## Ground rules

- Scratch repo: `~/Developer/giolt/glowc-phase0` (git, no commits, no remote).
- glowvm (`~/Developer/giolt/glowvm`) stays untouched.
- Local only. No `wrangler deploy` until the user says which Cloudflare account and region.
- Commits: no attribution lines (no `Claude-Session:`, no session URL, no `Co-Authored-By`).
- Don't `pkill workerd` broadly; it was denied. Only touch processes started from this dir.

## Layout

- `wat/*.wat`: probes (tailcall, plaincall negative control, features, eh_exnref, eh_legacy).
- `build.sh`: wat → `build/<name>.wasm` (wasm-opt re-encode) and `build/<name>.oz.wasm` (-Oz), validated.
- `checks/core`: tail calls, features, instance reuse (`/bump`), i64↔BigInt, `/env` (JSPI, EH globals, runtime compile).
- `checks/eh/{exnref,legacy}`: try_table vs legacy try, one Worker each.
- `checks/do`: DO SQLite, alarm, RPC between two DO classes.
- `probe.mjs <config> <paths...>`: start one worker via `unstable_startWorker`, fetch paths, exit within 60 s.
- `run-local.mjs`: runs everything (hung the first time, see below).

## Versions

- wrangler 4.159.0 (pinned exact in package.json), workerd 1.20261109.1.
- V8 version: TODO (read from `/env`).

## Findings so far

1. All probes build and validate. -Oz inlines the mutual recursion into one self
   `return_call` and inlines multivalue away, so the re-encoded build is the real test
   of those two; both builds are run.
2. workerd forbids `crypto.randomUUID()` at global scope ("Disallowed operation called
   within global scope"). Creating a `WebAssembly.Instance` at module scope was not
   rejected. Fix: make the isolate id lazily inside the handler.
3. Cause of the run-local.mjs hang: when the worker script throws at startup,
   `worker.ready` still resolves and `worker.fetch` then hangs forever. Every fetch
   needs a timeout.

## Checklist (Phase 0)

All local checks pass; details are recorded in SPIKE.md under Phase 0.

- [x] Versions: V8 15.8.62.5
- [x] Tail calls 10M (plaincall overflows at 10M, as expected)
- [x] Features with no v8_flags (exact wasm-opt output)
- [x] Module import, instance reused across requests (runtime compile is disallowed)
- [x] i64 ↔ BigInt
- [x] try_table and legacy try
- [x] JSPI present
- [x] DO: SQLite, setAlarm, RPC between classes
- [~] CPU time: wrangler dev shows wall time only; needs a deployed Worker
- [ ] Baselines (needs deploy: ask user)

## User decisions (2026-11-09)

- Deploy: yes. The login (the user's account) has several accounts; the user picked
  **Giolt** (`00000000000000000000000000000000`). Always deploy with
  `CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000`.
- Repo: keep local for now (no remote).
- OTP: use OTP 29 (installed); the doc's pin was changed from 28 to 29.
- oha: install via Homebrew.

## Deploy status (2026-11-09)

- `glowc-phase0-tail` deployed (Tail Worker + Store DO, records `cpuTime` per invocation).
- `glowc-phase0-core` deploy with `tail_consumers` was REJECTED: Giolt account is on
  Workers Free. "You must have a subscription to the Workers Paid plan to use Tail
  Workers" [code: 100150]. Nothing of core was deployed.
- Free also caps CPU at 10 ms per request, so glowvm `lustre_html` may die with
  `exceededCpu` rather than give a baseline. Waiting on the user: upgrade, other
  account, or Free with Workers Logs instead of a Tail Worker.
- Baseline settings chosen: glowvm commit 7190ebc dist copies, entry `index.min.js`,
  compatibility_date 2025-08-06 (what giolt's cloudflare_api.gleam uploads), no flags,
  `.avm` as a Data module.

## Benchmark notes

- CPU source on Free: `wrangler tail --format json` gives `cpuTime`/`wallTime` per
  invocation, in whole ms. It SAMPLES under load: at ~330 req/s it kept 66 of 10,000
  events, so that run (`*-r1-discarded-sampled.*`) is discarded.
- User asked for 5,000 measured requests (not 10,000), to protect the account's
  shared Free daily limit (~100k requests/day for all Workers on Giolt). The measured
  phase is rate-limited to 25 req/s (`oha -q 25`) so the tail keeps every event.
- Rate-limiting did NOT stop the sampling: at 25 req/s the tail still kept only ~8%
  (410 and 422 of 5,000). Run r1 numbers below are from that ~8% sample: provisional.
- Workers Logs has every event (head_sampling_rate 1), but wrangler has no query
  command and its OAuth scopes lack `workers_observability:read` / analytics read.
  Full-population p50/p99 needs either an API token (Account Analytics: Read, for
  GraphQL `workersInvocationsAdaptive` cpuTime quantiles) or reading the dashboard.
- Requests used today (approx.): 11,000 discarded run + 12,000 for r1 of both
  fixtures + probes ≈ 23,000. Each run costs 6,000. Keep the total well under 100k.

## Baseline results so far (glowvm 7190ebc, deployed, colo OTP)

| | basic_get | lustre_html |
| --- | --- | --- |
| Upload (total / gzip) | 823.58 KiB / 298.73 KiB | 860.82 KiB / 316.50 KiB |
| Worker Startup Time (wrangler deploy) | 31 ms | 16 ms |
| **CPU p50, median of 3 runs (Analytics, all requests)** | **4.13 ms** (4.13, 4.60, 3.86) | **4.02 ms** (4.25, 3.84, 4.02) |
| **CPU p99, median of 3 runs (Analytics, all requests)** | **13.94 ms** (12.21, 13.94, 13.99) | **13.41 ms** (13.41, 11.30, 15.79) |
| CPU p50 / p90 / p99 / max, r1 (8% tail sample, superseded) | 4 / 7 / 16 / 44 ms | 4 / 8 / 13 / 41 ms |
| Worker wall p50 / p99 | 4 / 17 ms | 4 / 15 ms |
| Client latency p50 / p99 (oha, 25 req/s) | 17.9 / 35.8 ms | 18.4 / 38.1 ms |
| Reference body sha256 | a93e435a… (17 B) | 24f3e3de… (81 B) |

CPU source (decided): Workers Analytics GraphQL `workersInvocationsAdaptive`, filtered by
`scriptVersion` (new scripts show as `__unknown__`) and the measured run's exact window
(`bench/analytics.mjs`). Counts are adaptive-sampled estimates (4,885–5,095 per 5,000 run).
Each run: 1,000 warm-up, then 5,000 measured at c=10, 25 req/s, colo OTP.

Memory (local instrumented copy, bench/glowvm-mem): 589,824 B (basic_get) and
655,360 B (lustre_html), the same on every request.
Cold start (bench/coldstart.sh, 5 deploys each): startup median 22 / 21 ms, first
request server time 81 / 68 ms, first-request CPU 5.6 / 4.7 ms (the 5th lustre sample
had no analytics row; median of 4). Each cold-start deploy made a new version, so the current
baseline versions are no longer 86dbb6fe / cf34b825; look up the latest before
querying analytics again.

Phase 0 is complete. All results are in SPIKE.md.

## Cleanup list

Everything created outside this repo. Add a line the moment something is created;
tick it when removed.

Cloudflare, account Giolt (`00000000000000000000000000000000`):
- [x] `glowc-phase0-tail`: deleted 2026-11-09 (Tail Workers need Paid; user chose Free + Workers Logs).
- [x] deleted 2026-11-09: `glowc-phase0-core` (https://glowc-phase0-core.SUBDOMAIN.workers.dev), observability on. `?n=` capped at 20M.
- [x] deleted 2026-11-09: `glowc-phase0-eh-exnref` (https://glowc-phase0-eh-exnref.SUBDOMAIN.workers.dev)
- [x] deleted 2026-11-09: `glowc-phase0-eh-legacy` (https://glowc-phase0-eh-legacy.SUBDOMAIN.workers.dev)
- [x] deleted 2026-11-09: `glowc-phase0-do` (https://glowc-phase0-do.SUBDOMAIN.workers.dev), DO classes Counter + Caller with SQLite data.
- [x] deleted 2026-11-09: `glowvm-baseline-basic-get` (https://glowvm-baseline-basic-get.SUBDOMAIN.workers.dev), observability on.
- [x] deleted 2026-11-09: `glowvm-baseline-lustre-html` (https://glowvm-baseline-lustre-html.SUBDOMAIN.workers.dev), observability on.
- Workers Logs from observability are kept by Cloudflare (retention per plan); they go with the Workers.
- Workers with Durable Objects (`glowc-phase0-do`) also own DO namespaces and
  SQLite data; `wrangler delete` removes them with the Worker.
- Remove with: `CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000 npx wrangler delete --name <worker>`

- [ ] User API token `glowc-phase0-analytics-read` (Account Analytics: Read, Giolt
      only, expires 2026-11-16). Created via the dashboard in the user's Helium browser.
      User decided: let it expire on 2026-11-16 (no revoke). Delete the local file after.

Local machine:
- [ ] Token file `~/.config/glowc-phase0/cf-analytics-token` (mode 600). Delete with the token.
- [ ] Helium remote debugging was turned on by the user (chrome://inspect/#remote-debugging);
      the user turns it off.
- [x] API-tokens tab closed and agent-browser detached (2026-11-09). Helium's own tabs untouched.
- [x] No `wrangler tail` processes left running (checked with ps).
- [ ] Temp files: `/tmp/glowc-tail.json`, `/tmp/glowc-tail.err`, `/tmp/bg*.log`, `/tmp/lh-*.log`.
- [ ] `oha` 1.16.0 installed with Homebrew (`brew uninstall oha` if not wanted after the spike).
- [ ] wrangler OAuth login for the user's account, stored in `~/Library/Preferences/.wrangler/config/default.toml`
      (the user ran `wrangler login`; `npx wrangler logout` revokes it, if wanted).
- [ ] wrangler logs under `~/Library/Preferences/.wrangler/logs/` (harmless).
- [ ] Earlier stray workerd processes from the hung run-local.mjs: none found on
      2026-11-09 (`ps` filtered to this dir), nothing to do.

In this repo:
- [ ] `run-local.mjs` hangs if a worker fails to start (no fetch timeout). Fix or
      delete it; `probe.mjs` replaced it.
- [ ] `bench/results/*.tail.json` and `phase0-core-deployed-tail.json` hold the client IP and
      `cf` geo data. Strip or delete them before the repo ever gets a remote.
- [ ] `build/` artifacts are committed on purpose (the exact bytes tested). Revisit
      if the repo gets a remote.

## Decisions after Phase 0 (2026-11-09)

- Probe and baseline Workers deleted. Baselines get redeployed from `bench/glowvm` at M5.
- Analytics token: kept until it expires on Nov 16.
- Heavy row: `json_roundtrip` carries 5×; `lustre_html` moved to the 3× fixed-overhead row.
- The user wants the whole spike done in one continuous run (not spread over a week),
  driven with /goal. The goal text is in GOAL.md. Keep going milestone by milestone.

## Spike run (goal in GOAL.md)

### Survey (tools/reach.escript, beam_disasm, OTP 29 / erts 17.1.1)

- Fixture app copied to `fixtures/app` (path dep on ../../../glowvm, read-only), built
  with gleam 1.19.2. glowvm's AtomVM shims (base64, io_lib_format, json, maps, rand,
  string, uri_string) are excluded; real OTP modules are used instead.
- Reachable from start/0: basic_get 142 functions / 22 modules / 7.9k instructions;
  lustre_html 181 / 28 / 9.4k; echo_body 162 / 24 / 13.4k; stdlib_string 697 / 37 / 36k.
  unicode_util is the biggest (3.6k instructions in basic_get).
- About 60 distinct opcodes; 24 to 74 BIFs per fixture (`erlang:is_builtin/3`).

### Architecture decisions (spike-pragmatic; deviations from SPIKE.md noted)

- Entry: compile the fixture's `start/0` unchanged. `host_io:read_stdin/0` and
  `write_stdout/1` are runtime overrides (request in, framed response out), so glowvm's
  bridge.gleam runs as is and responses are comparable byte for byte.
- glowrt (Rust, wasm32-unknown-unknown, owns memory) + generated app module importing
  glowrt functions and memory; linked with `wasm-merge`, then `wasm-opt`.
- Terms: 64-bit ERTS-style tagging (primary 2 bits, immediate 4/6 bits). Boxed headers
  with own subtags. Binaries: boxed [hdr, bitsize, byteptr, bitoff], bytes in a
  per-request arena (literals in a permanent area).
- Functions: one Wasm function per BEAM function, (x0..x(n-1): i64) -> i64; X regs are
  locals; Y regs in a linear-memory stack. call_last/call_only -> return_call.
  Result 0 = THE_NON_VALUE (exception); checked after every call.
- Control flow: forward-only label graphs -> nested blocks (block per label);
  functions with backward jumps -> loop + br_table dispatch.
- Exceptions: catch stack in the runtime (frame SP, handler id); a faulting function
  checks whether the top catch belongs to its frame, else restores SP and returns 0.
- GC: copying, roots = spilled live X regs + Y stack + exception state + pdict.
  Runtime allocation never collects (it chains a new block and sets a flag); collection
  happens only at test_heap/allocate_heap and function entry, where liveness is known.
- Literals: ETF blob decoded once per isolate at init (not per request). Deviation from
  "startup parses nothing"; counted in the startup measurement.
- Maps: sorted flat arrays at all sizes (no HAMT yet). Bignums via num-bigint.
- No processes/receive in tier 1 (receive paths only on error branches); Phase 2 work.
- Test host: Node 26 (V8, tail calls on by default). wasmtime not installed, not needed.

### json_roundtrip fixture (new) and its glowvm baseline

- `fixtures/app/src/json_roundtrip.gleam`: 335 records, 50,136 bytes of JSON decoded into
  typed records with gleam_json 3.1.0 and encoded again. Under real erl (`tools/erl-run.sh
  json_roundtrip`) the response is 200 and byte-identical to the input.
- glowvm runs it correctly but takes about 2.3 s per request under local `wrangler dev`
  (2475, 2327, 2342, 2289, 2290 ms). Deployed on Free (10 ms CPU limit) it will be killed,
  so there is no deployed glowvm number for the heavy row.
- Decision (made autonomously to keep the run going; flag in the final report): the heavy
  row compares both runtimes locally under the same workerd (`wrangler dev`, pure compute,
  wall ≈ CPU), same machine, same method. glowc is also deployed to show it fits the limit.
  One deployed glowvm attempt records the failure mode.

### Request budget

- 2026-11-09 used about 47k Worker requests on Giolt (over the 40k cap): no more deployed
  load today (UTC).
- M5 as written is 3 runs × 6k × 3 fixtures × 2 runtimes = 108k. Plan: reuse the Phase 0
  glowvm numbers for basic_get and lustre_html (same method, deployed 2026-11-09), so M5
  deploys only glowc for those (2 × 18k = 36k in one UTC day), and json_roundtrip is
  compared locally. Analytics token expires 2026-11-16.

### M1: core codegen (done 2026-11-09)

- Pipeline: `tools/glowc-build.sh OUT --entry M:F/0 --beams DIR` = cargo build glowc + glowrt,
  `glowc build` (app module importing glowrt), `wasm-merge` with glowrt, `wasm-opt -O3`.
  Runner: `tools/run.mjs` (Node 26). Difftest: `tools/difftest.sh M:F/0 --beams DIR` runs
  the same function under erl and compares with `=:=`.
- Passing vs erl (OTP 29): `m1:fib_test/0` (fib(30)), `m1:sum_test/0` (1..1,000,000 as a
  list), `m1:mixed_test/0` (tuples, bignums, try/catch, catch, lists, a fun), and the Gleam
  versions `m1_gleam:fib_test/0`, `m1_gleam:sum_test/0` (fixtures/app/src/m1_gleam.gleam).
- Local timings (Node, after warm-up): fib(30) 8.3 ms (erl JIT 6.5 ms); sum 4.8 ms
  (erl 9.1 ms). Instance init 2.1 ms.
- Risk found: body recursion uses the native V8 stack. Depth 10k passes, 100k overflows
  (`RangeError: Maximum call stack size exceeded`) under Node's default stack; that
  includes `lists:map` over 100k elements. Measure the workerd limit at M4. Not a kill
  criterion; fixtures stay far below it.

## User requests during the run

- 2026-11-09: "after you are done, create an html file so i can visualize and compare
  glowvm to glowc". Deliverable: `compare.html` (glowvm vs glowc: CPU p50/p99 per fixture,
  size, memory, cold start, targets from SPIKE.md's first table, verdict). Build it at
  the end of M5, from the measured numbers.

## Next steps

1. M2: binaries and strings (bs_create_bin, bs_match, unicode NIFs); stdlib_string,
   stdlib_bit_array, stdlib_string_tree vs erl.
