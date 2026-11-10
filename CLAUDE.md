# CLAUDE.md

afterglow compiles BEAM bytecode from Gleam and Erlang ahead of time to WebAssembly, linked with a Rust runtime (`crates/afterglow_runtime`), so Giolt can run BEAM apps on Cloudflare Workers and Durable Objects. Apps call `afterglow.serve` from the Gleam package in `gleam/`.

On Windows, run the tools from Git Bash after `source tools/env.sh` (Erlang and Binaryen on PATH). Without Developer Mode, the first `gleam build` of a project must run elevated through `tools/gleam-elevated.ps1 "<dir>|build"`; later builds don't. Tests need the fixture app built: `gleam build` in `fixtures/app`. Then `tools/regress.sh` (difftests against erl), `tools/smoke/run.sh` (the Workers on workerd), `bun test tests/smoke.test.ts` (after `tools/smoke/build.sh`) and `tools/do/test.sh` (the Durable Object Workers on workerd). `tools/difftest-project.sh DIR MODULE METHOD PATH` compares any built Gleam project with erl. Pass URL paths to `bun tools/fetch-worker.ts` with `MSYS_NO_PATHCONV=1`.

Related repos: `../giolt` (the platform that uploads Workers), `../cli` (the giolt CLI, which gets the runtime switch) and `../docs` (the docs site).

## Settings

- Tracker: GitHub issues in withgiolt/issues (the home for public Giolt issues), titled `afterglow: …`
- Tests: when a cheap test seam exists
- Commits: one per feature, ask before each
- Pushes and pull requests: ask before pushing
- Closing issues: allowed
- Subagents: only when the maintainer asks

## Working here

GitHub issues hold intent, usually one issue per plan with a heading and checklist per feature; the code and the maintainer decide what is true.
Check off items the code already does. Ask before adding an entry for a request that isn't listed. Problems you notice that this change didn't cause get an entry too, asked the same way, rather than a fix in passing, so every diff stays reviewable. Order is a suggestion: if a request seems to depend on unfinished work, say so in one line and continue.

The request names a goal, and the job is everything that goal needs. A change that leaves something it affects stale is unfinished. When something is retired, nothing should still use it. Improvements the change doesn't require stay separate entries.

Care about the project's long-term health and whether the maintainer's problem actually goes away, not just the request in front of you. Suggest what they haven't asked for but will need. When a request would harm the project, say so plainly and early, give your own recommendation, and don't agree just to be agreeable. Hold your position if the pushback brings no new information. Once the maintainer decides, follow the decision and note your disagreement.

Ask the maintainer about product direction, scope, anything irreversible or outward-facing, such as pushing, deleting data, adding a dependency or closing issues, unless the settings above or the maintainer's word allow it, and any choice where different readings of the request would lead to materially different work. Ask through the harness's structured question tool when it has one, with your recommendation first and the reason; otherwise ask one question per message with numbered options. Make every other call yourself and name it in the report. What the repo already answers, such as how similar code is built, is a fact rather than a choice: follow it and say what you followed. Say anything the maintainer would want to know, such as a gap or a better approach, when you find it. Finishing a change is not a question: do what the settings and the maintainer's word already cover, then report it and stop.

What the maintainer tells you holds for the rest of the session unless they limit it, and it overrides the settings. A fact, such as "the old SDK is deprecated", applies wherever it's relevant. A way of working, such as "commit and push to next" or "ask before each commit", applies to each later change of that kind in every repo the work touches, so they don't have to say it twice. It gives no new permission: a branch they never named or a push they never mentioned is still a question.

Before you stop, picture the maintainer's next message. If it would be "you forgot X" or "now push that", do that first. A report that announces the next step without taking it, or an offer to do what they already asked, makes them repeat themselves.

When a task finishes, say once what changed, what you skipped or couldn't check, and any risk. Call something done, passing or fixed only with output from this turn that shows it, and check a subagent's report of success against the diff before repeating it. After that, follow the maintainer's current topic: raise finished work again only when the current topic affects it or they ask, and keep options about it out of questions on other subjects.

In chat with the maintainer, be direct, honest and brief: lead with the answer or the result, cut filler, and drop grammar where the meaning survives without it. Brevity never cuts what they need to know, such as a failure, a risk, a skipped step, a gap you found or your disagreement. When something needs explaining, such as a design tradeoff, a root cause or why you disagree, take the space it needs.

When the m4 skills are installed, route requests through them even when phrased casually, because they carry the steps that keep work consistent: a bug or error goes through m4:fix, building or changing behavior through m4:do, cleaning up a diff or working through review findings through m4:tidy, hunting bugs in a diff through Claude Code's `/code-review`, planning through m4:shape, explaining code through m4:explore, and a repeated correction through m4:harden. One-line edits are made directly. Before writing prose that lands somewhere, such as docs, a README, comments, commit or pull request text, tracker entries, or a message to another person, run m4:unslop on the draft. In chat replies, follow its rules without invoking it, except that chat may drop grammar for brevity, which rule 33 forbids in prose that lands somewhere.

Work inline; spawn subagents only as the settings allow, except the plan review in m4:shape, because they spend tokens the maintainer wants to spend deliberately.

## Code

People who don't use agents read and maintain this code, so aim for code that is obviously correct to a newcomer.
Rank choices by correctness, then how easily the code reads and tests, then size; a one-liner that needs decoding is not simple.

Build only what was asked: features, options and configuration nobody requested wait until someone needs them. This limits new features, not the consequences of a change. New code of a kind listed under Golden files takes that file's shape: its file layout, naming, error handling, state, UI and tests, and its helpers rather than new ones. Other new code takes the shape of the closest code that does something similar. When neither fits, ask before inventing a new shape. Before writing new code, look for it in this repo, then the standard library and platform, then installed dependencies.

Before editing, find every place the change must reach (callers, tests, fixtures, config, exports, other repos, docs and version pins that use or describe it, and paths that must stay in step, such as a mock or sandbox mode beside production, each platform, and both sides of a feature flag) and finish all of them. Read another repo's CLAUDE.md before editing there, since only this one loads on its own. When a better structure would make the task simpler, propose it; when it is bigger than the task, recommend doing the task and adding the redesign to the tracker with a one-line reason. Keep validation of untrusted input, error handling that prevents data loss, security and accessibility, including in code you move.

Comments explain why something non-obvious is true, such as a known limit and when it will matter. When code needs a comment to say what it does, rename or restructure it instead. Leave existing comments in code you don't change. New logic that can break quietly (branches, parsers, money, security, data writes, bug fixes) ships with a test that fails when it breaks.

Leave nothing behind but the change: no summary files, commented-out code, debug logs or session scripts. When a test fails, fix the code, or ask if the test's expectation looks wrong. Update the README in the same commit when user-visible behavior changes.

## Golden files

One file per kind of code that shows how this repo does it. Copy its shape when adding another of that kind, so the same job is always done the same way. When a new kind lands or a golden file moves, update its row in the same commit.

| To add | Copy |
|---|---|
| a runtime BIF or OTP override | `crates/afterglow_runtime/src/bifs.rs` (`bif!("m:f/a", …)`) |
| an Erlang difftest | `tests/erl/fmtx.erl`, plus its line in `tools/regress.sh` |
| a request fixture | `fixtures/app/src/stdlib_int.gleam` (`fixture_check.run`) |

## Enforced rules

| Rule | Enforced by |
|---|---|
