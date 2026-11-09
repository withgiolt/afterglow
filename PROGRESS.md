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
| CPU p50 / p90 / p99 / max, r1 (8% tail sample) | 4 / 7 / 16 / 44 ms | 4 / 8 / 13 / 41 ms |
| Worker wall p50 / p99 | 4 / 17 ms | 4 / 15 ms |
| Client latency p50 / p99 (oha, 25 req/s) | 17.9 / 35.8 ms | 18.4 / 38.1 ms |
| Reference body sha256 | a93e435a… (17 B) | 24f3e3de… (81 B) |

Not yet measured: memory high-water mark, cold-start first-request timing.

## Cleanup list

Everything created outside this repo. Add a line the moment something is created;
tick it when removed.

Cloudflare, account Giolt (`00000000000000000000000000000000`):
- [x] `glowc-phase0-tail`: deleted 2026-11-09 (Tail Workers need Paid; user chose Free + Workers Logs).
- [ ] `glowc-phase0-core` (https://glowc-phase0-core.SUBDOMAIN.workers.dev), observability on. `?n=` capped at 20M.
- [ ] `glowc-phase0-eh-exnref` (https://glowc-phase0-eh-exnref.SUBDOMAIN.workers.dev)
- [ ] `glowc-phase0-eh-legacy` (https://glowc-phase0-eh-legacy.SUBDOMAIN.workers.dev)
- [ ] `glowc-phase0-do` (https://glowc-phase0-do.SUBDOMAIN.workers.dev), DO classes Counter + Caller with SQLite data.
- [ ] `glowvm-baseline-basic-get` (https://glowvm-baseline-basic-get.SUBDOMAIN.workers.dev), observability on.
- [ ] `glowvm-baseline-lustre-html` (https://glowvm-baseline-lustre-html.SUBDOMAIN.workers.dev), observability on.
- Workers Logs from observability are kept by Cloudflare (retention per plan); they go with the Workers.
- Workers with Durable Objects (`glowc-phase0-do`) also own DO namespaces and
  SQLite data; `wrangler delete` removes them with the Worker.
- Remove with: `CLOUDFLARE_ACCOUNT_ID=00000000000000000000000000000000 npx wrangler delete --name <worker>`

Local machine:
- [ ] `oha` 1.16.0 installed with Homebrew (`brew uninstall oha` if not wanted after the spike).
- [ ] wrangler OAuth login for the user's account, stored in `~/Library/Preferences/.wrangler/config/default.toml`
      (the user ran `wrangler login`; `npx wrangler logout` revokes it, if wanted).
- [ ] wrangler logs under `~/Library/Preferences/.wrangler/logs/` (harmless).
- [ ] Earlier stray workerd processes from the hung run-local.mjs: none found on
      2026-11-09 (`ps` filtered to this dir), nothing to do.

In this repo:
- [ ] `run-local.mjs` hangs if a worker fails to start (no fetch timeout). Fix or
      delete it; `probe.mjs` replaced it.
- [ ] `build/` artifacts are committed on purpose (the exact bytes tested). Revisit
      if the repo gets a remote.

## Next steps

1. Visual of the results (user asked). There is no glowvm-vs-glowc comparison yet:
   that needs the compiler (M5). Only local Phase 0 numbers exist.
2. Ask the user: deploy (account/region), repo location (withgiolt/glowc vs glowvm dir),
   OTP 28 install (29 is installed), oha install.
