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
- [x] `glowc-m5-basic-get`: created 2026-11-09 22:35 UTC (6 versions), deleted 22:39 UTC.
- [x] M5: `glowc-m5-basic-get`, `glowc-m5-lustre-html`, `glowc-m5-json-roundtrip` and the
      glowvm side-by-side Workers, deleted by bench/m5.sh after their runs (M5 notes above).
- [x] P2-P5: `glowc-p2-counter`, `glowc-p3-pingpong`, `glowc-p4-timers`, `glowc-p5-chat`,
      deleted by tools/do/deployed.sh after each check (Phase 2 deployed checks above).
      Not re-checked against the account from the Windows machine (no wrangler login there).
- Workers Logs from observability are kept by Cloudflare (retention per plan); they go with the Workers.
- Workers with Durable Objects (`glowc-phase0-do`) also own DO namespaces and
  SQLite data; `wrangler delete` removes them with the Worker.
- Remove with: `CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000 npx wrangler delete --name <worker>`

- [ ] User API token `glowc-phase0-analytics-read` (Account Analytics: Read, Giolt
      only, expires 2026-11-16). Created via the dashboard in the user's Helium browser.
      User decided: let it expire on 2026-11-16 (no revoke). Delete the local file after.

Local machine (the Mac that ran the spike; none of these exist on the Windows machine,
checked 2026-11-10, so they need doing on the Mac):
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
- [x] `run-local.mjs` deleted 2026-11-10 (it hung when a worker failed to start; `probe.mjs`
      replaced it).
- [x] `bench/results/*.tail.json` and `phase0-core-deployed-tail.json`: client IP, country
      and `cf` geo data stripped 2026-11-10 with `bench/strip-tail.mjs` (keeps colo and every
      measured field; event counts and CPU/wall sums checked unchanged). `bench/run.sh` now
      strips new tail files. **Git history still has the original files**: rewrite history
      (or start a fresh repo) before the repo ever gets a remote.
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

### M2: binaries and strings (done 2026-11-09)

- glowrt: `bits.rs` (bs_create_bin incl. in-place append on the arena, match contexts,
  old and OTP 26+ match ops, byte_size/bit_size on match contexts), `maps.rs` (flat
  sorted maps, put/get/has, maps BIFs, `erts_internal:map_next/3`), `binmod.rs`
  (binary:match/matches/split/compile_pattern/..., unicode:characters_to_binary/list).
  Codegen: `codegen_ext.rs`. Exceptions now carry `{M,F,A,[]}` frames added on unwind.
- Request difftest: `tools/difftest-req.sh MOD` (whole stdout byte for byte vs erl).
  PASS: basic_get, not_found, no_content, stdlib_string, stdlib_bit_array,
  stdlib_string_tree. Unit difftests `tests/erl/m2.erl` (8 tests) pass.
- Bugs found on the way: bs_get_tail must not move the match position; arena in-place
  append must compare exact (not rounded) ends.
- Known deviation: ERTS orders atom keys in small maps by atom index (e.g. #{a,b,c}
  iterates c,a,b on OTP 29); glowrt uses term order. =:= is unaffected; output that
  iterates an atom-keyed map can differ. Exception stack traces are coarser than ERTS.

### M3: maps, funs, exceptions, bignums (done 2026-11-09)

- PASS vs erl: stdlib_dict, stdlib_int, stdlib_function, stdlib_result, stdlib_dynamic;
  unit tests `tests/erl/m3.erl` (2^200 arithmetic and bignum division/shift/convert,
  closures, external funs, apply/2,3, named funs, fun_info, try/catch/after, nested
  raise, erlang:raise/3, function_clause, undef, 100-key maps).
- Added: time BIFs from a new host import `host.now_us` (Date.now; native unit = ns),
  erlang:md5 (md-5 crate), math NIFs, fun_info/2, io:put_chars/1,2 (appends to stdout
  like erl; host_io:write_stdout appends too).
- All 28 fixtures in fixtures/app now produce byte-identical output to erl under Node,
  including json_roundtrip (50,222 bytes) and lustre_html. Requests as in glowvm's
  smoke.test.ts (query_param `/?msg=hi`, not_found `/anything`, echo_body POST).
- Known gap: maps are flat sorted arrays at every size. ERTS switches to a HAMT above
  32 keys and iterates in hash order, so maps:to_list/fold over a >32-key map gives a
  different order than erl (lookups and =:= are the same). Not hit by any fixture.

### M4: bridge, GC, workerd (done 2026-11-09)

- Worker entry `tools/worker/index.js`: same request ETF and frame contract as glowvm's
  index.js, one Wasm instance per isolate reused across requests (glowrt resets heap,
  arena and process state in `rt_begin`), warm-up request at module scope, instance
  recreated after a trap. Env vars: host writes `K=V\0` into `rt_env_buf`, read by
  `os:getenv/0,1`. Build: `tools/build-worker.sh FIXTURE OUTDIR`.
- Deviation from SPIKE.md M4 wording: no separate `handle_request` export or Rust bridge;
  the fixture's `start/0` and glowvm's bridge.gleam run compiled, with host_io as runtime
  overrides. Responses are byte-identical, which is the M4 gate.
- Smoke on workerd (`tools/smoke/run.sh`, one workerd, every fixture on both runtimes,
  glowvm side = glowvm's own fixtures/app/dist, read-only): all 30 requests from
  glowvm's smoke.test.ts identical in status, headers and body (basic_get, not_found,
  env_get, query_param, bytes_body, echo_body ×3 incl. 300 KB, no_content, lustre_html,
  file_body, json_roundtrip, 18 stdlib_* fixtures).
- GC: rewritten to a chained Cheney to-space sized from the last live size (it used to
  double every cycle), and arena compaction: once 8 MB of binary bytes were allocated
  since the last GC, a GC is requested and live byte ranges move to a fresh arena.
  `fixtures/app/src/gc_churn.gleam`, 500 MB of garbage in total:
  list cells: glowc 0.20 s, 5.4 MiB Wasm memory (5,997 GCs); glowvm 18.1 s.
  binaries: glowc 0.11 s, 14.9 MiB; glowvm 0.11 s. Both inside 128 MB.
- Recursion depth on workerd (body recursion `1 + depth(n - 1)`): glowc fails above
  about 12,340 frames (`RangeError: Maximum call stack size exceeded`, 502, the next
  request works again). glowvm handles 1,000,000 (33 s). **This is a real regression
  against glowvm**, e.g. `lists:map` over ~12k elements. Not a kill criterion; candidates:
  smaller frames, growable stacks (JSPI), or a CPS fallback for deep non-tail calls.
- `tools/regress.sh`: 49 difftests vs erl (M1–M3 unit tests, all fixtures, gc_churn).
- Port note: Helium's CDP listens on 9222, so the harness uses 9400+/9500+.

### M5: benchmark gate (done 2026-11-10: pass, verdict in SPIKE.md)

- Size pass (ccd46bb): only literals used by reachable code are emitted, Y stack 16K
  words, Rust shadow stack 64 KiB, smaller GC blocks. sizes below. Size row: pass.
- Memory, per-isolate high-water over 5,000 requests through one reused instance under
  `wrangler dev` (out/memprobe): basic_get 512 KiB, lustre_html 512 KiB (first request =
  max) vs glowvm 576 / 640 KiB. Memory row (its stated fixtures): pass.
  json_roundtrip: glowc 4,992 KiB vs glowvm 1,280 KiB (bench/glowvm-mem/json_roundtrip,
  same probe). glowc is 3.9× higher here: the 2 MiB nursery from the profiling pass
  (live sets are at most 24K words). A nursery sweep (out of tree) bottomed out at about
  2.5 MiB and cost up to 2× time (2 blocks: 6.29 ms vs 3.01 ms median under Node), so the
  measured build keeps 2 MiB. Flag in the verdict.
- Upload sizes, `wrangler deploy --dry-run` gzip: glowc 180.97 / 187.50 / 205.73 KiB,
  glowvm 298.73 / 316.50 / 308.37 KiB (basic_get / lustre_html / json_roundtrip).
- Profiling pass (112c2df): GC nursery sized from the live set, inline bs_match fast
  paths, name sections behind GLOWC_NAMES=1, `tools/profile.mjs`. json_roundtrip under
  Node: median 5.6 ms -> 3.4 ms, GC share 32% -> about 7%.
- Local workerd, same machine, oha c=1 (`bench/local.sh`, wall time ≈ CPU), before the
  profiling pass: basic_get glowvm p50 0.81 / p99 3.66 ms, glowc 0.13 / 0.40 ms;
  lustre_html glowvm 0.98 / 3.45, glowc 0.15 / 0.45; json_roundtrip glowvm 2644 / 2872 ms
  (n=20), glowc 6.53 / 15.58 ms. After the profiling pass (2026-11-09 22:50, raw in
  bench/results/local/): basic_get glowvm 0.86 / 3.67, glowc 0.14 / 0.71; lustre_html
  glowvm 1.02 / 4.02, glowc 0.14 / 0.39; json_roundtrip glowvm 2367.73 / 2788.44 (n=20),
  glowc 3.78 / 4.61 ms (626× at p50). Smoke on workerd: 30/30 identical.
- Deployed measurement: `bench/m5.sh <dir>` = 5 cold-start deploys (`coldstart.sh`),
  3 × (1k warm-up + 5k at c=10, 25 req/s) on the last version (`measure.sh`), CPU from
  Analytics (`analytics.mjs`), first-request CPU per deploy (`coldcpu.mjs`, checked
  against the Phase 0 glowvm CSV: median 5.61 ms, same as recorded), then delete.
  Worker dirs: `bench/glowc/<fixture>` (built with tools/build-worker.sh).
- Check when json_roundtrip lands: glowvm's deployed CPU was about 4.8× its local wall
  time, which would put glowc's json_roundtrip near 18 ms deployed, over Free's 10 ms
  limit (not strictly enforced: glowvm's p99 reached about 14 ms with no failures).
  Report the non-success rows (exceededResources and the like) from analytics before
  claiming this workload fits Free. The heavy row itself is judged locally.
- 2026-11-09 22:35 UTC, on the user's request ("don't wait for it please, go ahead"),
  the basic_get and lustre_html runs started without waiting for the UTC day: the
  account was at 50,338 requests that day (47,153 from spike Workers), so 2026-11-09
  ends near 86k, over the goal's 40k cap but under Free's 100k. json_roundtrip waits
  for 2026-11-10 UTC so the day stays under 100k.
- 2026-11-09 22:35 UTC run aborted: from the first request, every glowc-m5-basic-get
  response was `404`, body `error code: 1042` (5 cold-start requests, 1 check, 1,000
  warm-up, then stopped; about 1,006 requests). A redeploy with `"workers_dev": true`
  gave the same. Production Workers on workers.dev (`gote`, `giolt-next`) returned the
  same 404/1042 at 22:38 UTC. glowc-m5-basic-get deleted 22:39 UTC.
- Checked in the dashboard (user's Helium, own tab, closed after): production runs on
  custom domains (giolt.com 200, dash.giolt.com 307, gote.giolt.app 200), so `gote` and
  `giolt-next` simply have no workers.dev route; the account showed 50,346 / 100,000
  requests. Redeployed glowc-m5-basic-get: the dashboard listed its workers.dev URL as
  enabled, and it answered 1042 for about 3 minutes, then 200. Cause: a new workers.dev
  hostname takes minutes to serve. `bench/m5.sh` now deploys, waits for a 200 (one
  probe per 15 s), and refuses to load a Worker whose cold-start requests were not 200.
  Runs restarted at 22:45 UTC.
- Deployed results, 2026-11-09 22:45 to 23:20 UTC (both Workers deleted; all 30,000
  measured requests 200, no errors in analytics; deployed responses identical to
  glowvm's). CPU p50 / p99 ms, runs 1-3:
  - basic_get 0.483 / 2.424, 0.456 / 2.505, 0.462 / 2.366. Median 0.462 / 2.424 against
    ≤ 1.38 / 4.65. PASS (8.9× / 5.8× below glowvm's 4.13 / 13.94).
  - lustre_html 0.560 / 2.782, 0.554 / 2.777, 0.545 / 2.862. Median 0.554 / 2.782
    against ≤ 1.34 / 4.47. PASS (7.3× / 4.8× below glowvm's 4.02 / 13.41).
- Cold-start series from those runs is not usable. Deleted Workers show as
  `__unknown__` in analytics, so the series was checked per version account-wide. All
  five basic_get cold requests were served by an older version (0e9b98c6, the readiness
  deploy), not the freshly deployed one. Two of the five lustre_html requests were
  missed the same way. A 5 s wait after deploy is too short: a new version can take over
  a minute to reach every edge. The Phase 0 glowvm series passes the same check (each
  version exactly 1 request). With all 5 samples, glowvm lustre_html first-request CPU
  is 4.91 ms, not the 4.7 ms (n=4) in SPIKE.md. `coldstart.sh` now waits 60 s
  (COLD_WAIT). `coldcpu.mjs` keeps only versions with exactly one request. 2026-11-09
  23:50 UTC: new 10-sample series for glowc and glowvm, both fixtures, side by side.
- Cold-start results, side by side, 10 deploys per Worker, 60 s wait (`*-cold10.csv`,
  `*-cold10.cpu.json`). The tenth sample of each served the following load run, so it
  is excluded. One glowc lustre_html sample had no request on its version. Medians:

  | | startup | first-request server time | first-request CPU |
  | --- | --- | --- | --- |
  | basic_get glowc | 6.5 ms | 32.8 ms | 2.06 ms (n=9) |
  | basic_get glowvm | 20 ms | 44.7 ms | 3.68 ms (n=9) |
  | lustre_html glowc | 6.5 ms | 32.5 ms | 3.09 ms (n=8) |
  | lustre_html glowvm | 21.5 ms | 55.4 ms | 5.91 ms (n=9) |

  Startup row: PASS on all three measures for both fixtures.
- **Incident, 2026-11-09 23:47 UTC: the account went over Free's 100k requests for
  the day.** `m5.sh <dir> 0` was meant to run no load. On macOS, `seq 1 0` prints 1 and
  0, so all four cold-series Workers started a 6k-request run (1k warm-up + 5k) at
  once. I saw it at 23:51 and killed the runs. All four Workers were deleted (their
  hostnames answer 1042). By analytics, 2026-11-09 ended at about 110,000 requests: the
  goal allows 40k, and the earlier override at the user's request had planned about
  87k. Production checked at 23:52: giolt.com 200, dash.giolt.com 307,
  gote.giolt.app 200. Fixed: m5.sh counts with an arithmetic loop. The three completed
  runs are kept as `*-sbs1` (one side-by-side run): basic_get glowc 0.412 / 2.259 vs
  glowvm 3.294 / 6.485 ms p50 / p99 (8.0× / 2.9×), lustre_html glowc 0.570 / 2.769.
  The glowvm lustre_html run was cut short and has no result. Production analytics
  for 23:40-00:05 UTC: 7 requests, all successful (giolt, giolt-dash, giolt-dash-next,
  gote). Requests refused at the daily limit wouldn't show up there.
- 2026-11-10 (day total 24,482 requests at 00:36 UTC, account-wide):
  - json_roundtrip, glowc, 3 runs, all 15,000 measured requests successful: p50 / p99
    7.473 / 13.451, 5.922 / 12.187, 7.637 / 12.894 ms, so median 7.47 / 12.89. That is
    over Free's nominal 10 ms at p99, with no failures. Cold start (1 deploy): startup
    14 ms, server 61.2 ms.
  - json_roundtrip, glowvm, deployed (`bench/probe-once.sh`): 3 of 3 requests got 503
    `error code: 1102` (CPU limit exceeded) after about 2.1 s. glowvm can't serve the
    fixture deployed, so the heavy row compares glowc's deployed success with local
    numbers (626× at p50).
  - json_roundtrip correctness: glowvm and glowc on local workerd answer identically
    (50,136 B, `bench/results/json-roundtrip-local-compare.txt`).
  - glowvm lustre_html side by side, 1 run (`*-sbs1`): 4.864 / 7.947 ms vs glowc 0.570
    / 2.769 (8.5× / 2.9×). With basic_get's 2.9×, glowvm's same-day p99 is about half
    its Phase 0 value. The verdict uses the Phase 0 thresholds (n=1 here); the 2.9× is
    stated in the verdict.
- Verdict (SPIKE.md, Results): pass on every row, no kill criterion.
- First-request cost, local: with the flag warm-up (glowvm's contract, which glowc
  keeps), the first real request runs handler code V8 has not compiled yet. Under Node
  that costs about 0.9 ms more than later requests; a warm-up through the real handler
  removes it (0.03 ms). glowvm's warm-up skips the handler too, so the gate build keeps
  the flag warm-up. A full-handler warm-up would also run user code at global scope,
  where workerd refuses random values.
- Schedule (superseded by the runs above), as first planned: 2026-11-10 UTC glowc basic_get + lustre_html (36,010 requests).
  2026-11-11 UTC glowc json_roundtrip (18,005), glowvm basic_get and lustre_html
  redeployed for one side-by-side run each (12,010), one glowvm json_roundtrip request
  to record the failure mode.

### P1: in-request scheduler (done; local)

Local only; the M5 Worker binaries in bench/glowc are from af5e8d0 and were not rebuilt.

- glowrt `sched.rs`: processes inside one request. spawn/1,3, spawn_link, spawn_monitor,
  spawn_opt/2, `!`/send, selective receive (loop_rec, loop_rec_end, remove_message, wait,
  wait_timeout, timeout; recv markers are no-ops), links, monitors (DOWN), exit signals
  (kill/killed, normal ignored unless trapped, trap_exit 'EXIT' messages), exit/2,
  register/whereis/unregister, is_process_alive, process_info/2 (a few items).
  proc_lib:spawn_link/1,3 are runtime overrides (no proc_lib init_p, no $ancestors).
- Suspension as SPIKE.md describes: a `wait` stores a resume record (table index, arity,
  dispatch block, entry sp, sp) and returns the marker; each caller on the way out
  that may resume after the call pushes its own record. The generated `main` is now a
  scheduler loop around `rt_sched`, re-entering the top record's function through the
  table with `g_res` = block + 1. Whole-program analysis marks functions that may
  suspend; only non-tail call sites to those get a resume label (a dispatch block), so
  the normal path is unchanged. json_roundtrip: 3.01 -> 3.02 ms median (Node);
  app.wasm gzip +5% (basic_get 182 -> 193 KB), mostly from the reachability change below.
- All processes share the request heap (messages are not copied). Each spawned process
  gets its own 64 KiB Y stack. Receive timeouts use virtual time: they fire only when no
  process can run.
- Deviation: no preemption. A process runs until it waits or exits. A process that spins
  without receiving starves the rest. Preempting at arbitrary call sites would make every
  call site resumable; not worth it for request-scoped code.
- Reachability: spawn/apply(M, F, A) with M and F moved in as atoms right before keeps
  every exported M:F reachable (else a warning). Added raw_raise and erlang:is_integer/3
  (needed by logger code that this pulls in).
- Fixed on the way: a fault in a block laid out after its try handler's label jumped
  backward without dispatch (gleam_erlang_ffi:register_process/2).
- Tests vs erl: p1:actor_test (gleam_otp actor answering process.call), p1:pingpong_test
  (100,000 messages), tests/erl/p1e.erl (basic, links, monitors, timeouts, selective
  receive, registry). regress: 57/57. Smoke on workerd: 30/30.

### P2: checkpoint and restore (done; deployed PASS 2026-11-10)

- glowrt node mode (`rt_node_mode`): heap, arena and processes persist across turns;
  each turn gets a fresh main process on the main stack. Dirty tracking per process
  (ran, got a message, link/monitor/name changed).
- Checkpoint (`rt_checkpoint`): one ETF row per dirty process: status, resume records
  (sps as offsets from the stack top), the Y stack from the lowest frame (non-term words
  wrapped), mailbox, save pointer, dictionary, links, monitors, trap_exit, names, start
  closure, and the last gen-style state {Mod, State}. Plus a meta row (next pid, ref
  counter, virtual clock). Exited processes are reported for deletion.
- Restore (`rt_restore(upgrade)`): same build: the process comes back with its frames
  and resumes exactly where it waited. Another build: gen-style processes restart in
  `glowc_node:upgrade/3` (Mod:code_change, then gen_server:enter_loop under the same
  name, which stays registered across the restore); the rest exit with code_upgrade.
- `tests/erl/p2/glowc_node.erl`: turn entry ({call, Name, Mod, Msg} in, reply out) and
  the state snapshot (sys:get_state of every dirty process whose '$initial_call' is
  {Mod, init, 1}). `tools/node-harness.mjs`: a simulated DO (rows in a Map tagged with
  the build hash; evict = drop the instance).
- Tier-2 builds need `--keep counter --keep gen --keep gen_server --keep glowc_node`
  (proc_lib and the upgrade path reach them only through computed apply).
- Added on the way: process aliases (monitor/3 alias option, alias/0,1, unalias/1; gen
  replies go to the alias), erlang:spawn_opt/4, erlang:make_fun/3, the update_record
  opcode (tests/erl/ur.erl), reachability through spawn_opt/4.
- Fixed: a restored process lost its dictionary (new_turn saved the empty global one
  into it).
- `tools/p2-test.sh` (in regress): {v1,1} {v1,2} evict {v1,3} redeploy {v2,4} {v2,4}
  evict {v2,5}: PASS. regress 59/59, smoke 30/30.
- Not done: local funs in checkpoints (ETF NEW_FUN_EXT is not encoded; such a process
  logs and is not checkpointed), stack traces from proc_lib crash reports (badmatch in
  proc_lib:my_info_1 on that error path).
- Durable Object host `tools/do/node.js` (class GlowNode, SQLite table procs(pid,
  build, bytes), build hash baked in by `tools/do/build-do.sh`). Local workerd
  (`tools/do/local-test.mjs`, wrangler dev with persisted storage; restart = eviction,
  v2 restart = redeploy): {v1,1} {v1,2} {v1,3} {v2,4} {v2,4} {v2,5}, rows of about
  480 bytes per turn. PASS.
- Next: one deployed check on Giolt (`glowc-p2-counter`, about 10 requests) after the
  day's M5 runs, then P3.

### P3: messages between DOs (done; deployed PASS 2026-11-10)

- Remote pids: the node's atom index sits in bits 36.. of the pid word; ETF carries the
  node name (decoded back to a local pid when it names this node). `erlang:node/0,1`
  return the DO's name, set by the host (`rt_set_node`).
- A send to a remote pid or to `{Name, Node}` goes to the outbox. After the turn the DO
  host sends one `{deliver, [{Dest, Msg}]}` turn per target DO through its stub, in
  order per target (a promise chain under ctx.waitUntil, not awaited by the turn, so
  ping-pong never nests DO calls). If delivery fails (the target throws or is
  unreachable), the sender runs `{noconnection, Node}`.
- Monitors on remote processes are stored on the target's side (a `$glowc_monitor`
  control message; noproc comes back if the target is missing). `noconnection` fires
  'DOWN' with reason noconnection for every monitor on that node.
- Not done: links across DOs, remote monitors in checkpoints (lost if the watcher's DO
  is evicted), at-most-once only (no retries).
- `tests/erl/p3/pp.erl`, `tools/do/p3-test.mjs`: 500 round trips a -> {ponger, b}
  (1,000 messages) in 787 ms on local workerd (1.6 ms per round trip; 200 round trips:
  4.0 ms each), then a remote monitor fires noconnection after b throws on purpose
  (`glowc:node_crash/0`). PASS. P2 DO test, regress 59/59 and smoke 30/30 still pass.

### P4: timers (done; deployed timer PASS 2026-11-10, cron sparse)

- glowrt: erlang:send_after/3,4, start_timer/3,4 ({timeout, Ref, Msg}), cancel_timer/1,2,
  read_timer/1,2. Tier 1: virtual clock (a timer fires when nothing can run, before a
  later receive timeout); `tests/erl/p1t.erl` matches erl. Node mode: wall-clock
  deadlines; the timer table is part of the meta checkpoint row.
- DO host: after each turn one alarm for the earliest timer (`rt_next_timer`); `alarm()`
  runs a `timers` turn (every turn first fires the due timers). The DO's name is kept in
  SQLite (alarms have no request). Cron: the Worker's `scheduled` sends {tick, Cron} to
  GLOWC_CRON_PROC in DO GLOWC_CRON_NODE (`CRON=... tools/do/build-do.sh`).
- `tests/erl/p4/tm.erl`, `tools/do/p4-test.mjs`: a 60 s timer armed, Worker restarted
  with persisted storage (instance gone), no request for 65 s: the alarm woke the DO,
  the restored process got its message at 60,020 ms. Two Cron ticks reached `ticker`
  and survived the restart. PASS. Receive `after` timeouts stay virtual in node mode
  (they do not arm alarms): a deviation.
- regress 60/60, P2 and P3 DO tests still pass.

### P5: WebSockets (done; deployed PASS 2026-11-10)

- DO host: `Upgrade: websocket` -> `ctx.acceptWebSocket` (Hibernation API). A socket is
  `{glowc_ws, Id}` (id from a persisted counter), its owner process name and module in
  the socket attachment. Events reach the owner as `{websocket, Ref, open | {text, Bin}
  | {binary, Bin} | closed}` through a `{ws, Name, Mod, Ref, Event}` turn (the owner is
  started as Mod:start() if missing). `glowc:ws_send/2` (binary or iolist) and
  `glowc:ws_close/1` queue output that the host sends after the turn.
- `tests/erl/p5/chat.erl`, `tools/do/p5-test.mjs`: two clients; after "hi", the DO
  instance is dropped with the sockets open ("__evict", local stand-in for
  hibernation), then "yo" restores the room from SQLite: both clients get
  "2: yo (members 2, messages 2)", and the restore is reported ({"restored":2}). PASS.
  Deployed: wait past the hibernation idle time instead of "__evict".

### Phase 2 deployed checks (2026-11-10, `tools/do/deployed.sh`, each Worker deleted)

- P2: {v1,1} {v1,2}; after 180 s idle {v1,3} with boot {"restored":2}; redeploy to v2,
  then {v2,4} with {"restored":2,"upgraded":2} (code_change), {v2,4}; after 180 s idle
  {v2,5} restored. Rows 472 / 463 B. PASS.
- P3: first run stalled. With a debug build logging each send's depth, the 11th hop
  between the two DOs never arrived, and nothing was logged: Cloudflare caps how deep
  a chain of requests between Workers and DOs can go. Fix in `tools/do/node.js`: each
  send carries `x-glowc-hop`, and past 8 hops the outbox goes to an SQLite queue (outq)
  that the DO alarm sends, which starts a new chain. A node with queued sends queues
  later ones too, to keep order per sender and receiver. Deployed after the fix: 200
  round trips at 72.8 ms each, 4 round trips (no relay) at 77 ms each, so the relay adds
  nothing measurable. `noconnection` fires. PASS. Local: 1.47 ms per round trip.
- P4: armed a 60 s timer, then 200 s with no requests. The result call found the
  timer fired, from a restored instance ({"restored":3}). That run didn't record the
  fired-after value, so it can't separate the alarm firing at 60 s from a lazy fire.
  Cron ticks: 0. Rerun with 600 s idle (`bench/results/deployed-p4-idle600.log`): the
  timer fired 60,002 ms after arming (decoded from the reply), read from a restored
  instance ({"restored":3}), so the alarm woke the evicted DO on time. PASS for the P4
  test. Cron: 1 tick ({tick, "* * * * *"}) in 600 s, where about 10 were due. Delivery
  works, but deployed cron ran far less often than its schedule; not explained.
  No requests reached the DO during the wait, but eviction before the 60 s mark
  is shown only by the local test, which forces the dispose.
- Builds: deployed P2 and P5 ran the `tools/do/node.js` from before the hop relay
  (1317de8). P3's passing run and the P4 rerun ran the relay version. The baked-in
  BUILD hash covers only app.wasm, so it doesn't tell them apart. After the deployed runs,
  the relay also keeps the alarm at "now" while any send is queued, not just one
  queued in that turn (otherwise a pending timer could push it out); local P2-P5 pass.
- P5: two WebSocket clients, 30 s idle between messages, room state restored
  ({"restored":2}). PASS.

## Real-app test: gote (2026-11-10, Windows)

- Toolchain now also runs on Windows (Git Bash): `source tools/env.sh` (Erlang erts bin,
  Binaryen 133), rebar3 3.28.0 in ~/.local/bin, `.gitattributes` keeps LF. Gleam's
  Erlang-target builds need symlinks: without Developer Mode, the first build of a
  project runs elevated through `tools/gleam-elevated.ps1` (one UAC prompt); later
  builds work unelevated.
- glowvm's own test suite (`scripts/smoke.test.ts`) ported unchanged to
  `tests/smoke.test.ts`, run against glowc Workers: 29/29.
- The giolt dashboard can't be tested: it is a JavaScript-target app (plinth, node_pg,
  glen, gleam_fetch), so there is no BEAM code to compile.
- gote (local branch `afterglow-stress` in ../scratch/gote, not pushed): glowvm dep moved
  to 7190ebc (gote pinned 03136a0, which still used the JSON request protocol), and
  `/stress` added: 40 checks, one Erlang/OTP feature each, compared with real erl by
  `tools/difftest-project.sh`. Original routes work on workerd (home, /status.json,
  worker_sum 108). Stress: 34/40 identical to erl. Remaining gaps:
  - not implemented: ETS, crypto hash/hmac (crypto:hash/mac NIFs; Wisp signed cookies
    and sessions use HMAC-SHA256, so apps using them crash), regexp (re NIFs);
  - erlang:phash2 uses glowrt's own hash, not ERTS's;
  - small atom-keyed maps iterate in a different order (known);
  - deep_recursion_100k: stack overflow traps the instance (502, next request fine).
- Fixed on the way: process_info/2 list form and more items; crash reports from
  proc_lib (logger:macro_log, error_logger depth/limit_term overrides print one line to
  the host log); reachability for OTP behaviours (proc_lib:start*, gen_server/
  supervisor start with a literal callback module, binary_to_atom on a literal module
  name, gen/gen_server/gen_statem/gen_event/supervisor kept whole once reached);
  init:get_arguments/0 (erl_features asks on the first atom printed: every ~p/~w of an
  atom failed); float short format (2500.0 -> "2.5e3", ERTS's shorter-form rule);
  new BIFs: get_keys, pid/ref/fun_to_list, system_info/1, processes/0,
  process_info/1, statistics/1, localtime/universaltime/date/time,
  io:printable_range/0, string:list_to_float/1. glowc-build.sh takes the newest OTP lib
  when several are installed.
- Sizes (gzip): basic_get 214 KB, lustre_html 220 KB, json_roundtrip 237 KB, stress gote
  572 KB.
- Tests are faster: regress runs in parallel without wasm-opt (62 tests, ~60 s instead of
  ~195 s); the depth test stays optimised; smoke builds in parallel with -O3.

## User requests during the run

- 2026-11-09: "after you are done, create an html file so i can visualize and compare
  glowvm to glowc". Deliverable: `compare.html` (glowvm vs glowc: CPU p50/p99 per fixture,
  size, memory, cold start, targets from SPIKE.md's first table, verdict). Build it at
  the end of M5, from the measured numbers.

## Next steps

1. Optional: two more glowvm side-by-side runs per fixture (2026-11-11 or later) to
   firm up the 2.9× p99 ratio.
2. Cleanup list above: only the Mac-side items and the git-history rewrite are left.
