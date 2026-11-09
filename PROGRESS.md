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

- Deploy: yes, to the default account from `wrangler whoami`. Show the account before the first deploy.
- Repo: keep local for now (no remote).
- OTP: use OTP 29 (installed); the doc's pin was changed from 28 to 29.
- oha: install via Homebrew.

## Next steps

1. Visual of the results (user asked). There is no glowvm-vs-glowc comparison yet:
   that needs the compiler (M5). Only local Phase 0 numbers exist.
2. Ask the user: deploy (account/region), repo location (withgiolt/glowc vs glowvm dir),
   OTP 28 install (29 is installed), oha install.
