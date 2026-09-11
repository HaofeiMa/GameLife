# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

GameLife — a macOS tray app (Tauri 2 + React + TypeScript + Rust) that estimates "effective main-line research time" in 15-second samples, then pays out energy/coins for it. Runs tray-only: closing the window hides it, only tray → 退出 exits the process. UI strings and all specs/plans are in Chinese; commit messages are English conventional commits (`feat:`, `fix:`).

Data root: `~/Library/Application Support/GameLife/` (`gamelife.db`, `config.json`, optional `screenshots/`). The OpenAI API key lives only in the macOS keychain (service `ma.haofei.gamelife.openai`, account `GameLife`) — never in `config.json`.

## Commands

```bash
npm install
npm run tauri dev            # run the app (tray + window)
npm run tauri build          # release bundle

cargo test --offline -p gamelife-core        # pure domain logic (fast, no DB/OS)
cargo test --offline -p gamelife             # DB, scheduler, macOS, vision
npm test                                     # vitest (frontend)

# single test / module filter
cargo test --offline -p gamelife-core judge::
cargo test --offline -p gamelife scheduler::tests::tick_capture_skips_never_capture_app
npx vitest run --dir src lib/format   # --dir src keeps .worktrees copies out
```

`--offline` is the README's convention (all deps are in `Cargo.lock` + local registry cache); drop it if you actually need to fetch.

**Any change under `src-tauri/` must be validated with `cargo test -p gamelife`** that compiles *and* runs — `--no-run` is not sufficient. This is a standing constraint from the Wave 1 plan.

The checkout was moved from `/Users/mahaofei/Projects/GameLife` to this path, and `target/` cached build-script output keyed to the old absolute path — that made the `gamelife` build script fail with a `permission files` error under the dead directory. It was cleared once by deleting those `target/debug/build/*` dirs; if it recurs after another move, do the same.

`npm test` scans the whole repo, so it collects the test copies inside `.worktrees/` as well as `src/` (vitest does not exclude that directory) and the counts it prints include those duplicates. Scope a run with `--dir src`.

## Architecture

A two-crate Cargo workspace with a hard layering rule:

- **`crates/gamelife-core`** — pure domain logic, no I/O. Dependencies are only `chrono`/`serde`/`serde_json`: no `rusqlite`, no `tauri`, no filesystem. Holds the classification rules (`hint`), span arithmetic (`observe`), the slot decision (`judge`), reward keys (`ledger`), `streak`, `shop`, `weekly`, `early_start`, `policy`, `vision_ctx`. Everything here is unit-testable in-process; keep it that way.
- **`src-tauri`** — the app shell. `db.rs` (schema + `PRAGMA user_version` migrations), `macos.rs` (all OS observation: `osascript` for frontmost app/title/document path, CoreGraphics FFI for idle/lock, window capture), `sampler.rs` (the 15s loop behind a `SampleSource` trait so tests inject a fake), `scheduler.rs` (the orchestrator, ~3k lines: slot lifecycle, random capture scheduling, retention purging, policy seeding, finalize, settle, midnight), `resolve.rs` (one transaction writing `JudgeOutput` into `slots` + `ledger`), `commands.rs` (the Tauri command surface), `vision.rs` (OpenAI HTTP).

Data flow: sampler tick → `samples` row + `heartbeat`; scheduler `ensure_slot` / `tick_capture` (screenshot at a random 4–13 min offset into the slot, persisted as `capture_scheduled_at`) → at slot end `hints_for_slot` → `judge_slot` (core) → `resolve_slot` (ledger).

Frontend: `src/pages/*` per tab, `src/lib/api.ts` is the single place that calls `invoke()`; `src/lib/format.ts` mirrors core formatting.

## Invariants that are easy to break

These come from the specs and are enforced by tests — read the relevant spec section before changing judgement, capture, or ledger code.

- **No extrapolation across gaps.** Adjacent samples more than `2 × 15s` apart leave the middle `unobserved`; never fill it from the endpoints. Always `credited ≤ observed ≤ actual slot duration`.
- **Process death is `unobserved`, never `away`.** Exit / crash / force-quit / reboot gaps become `unobserved` with credited 0. Pause while the process lives is `break_away`. A cross-midnight restart only backfills to 24:00 of the heartbeat's own day.
- **Final/unknown slots are immutable.** V0.1 only allows reporting a misclassification — no retroactive economic correction.
- **Ledger writes are idempotent** via `UNIQUE reward_event_key`. Only `DbOpError::AlreadyApplied` may be swallowed; IO/FULL/CORRUPT must surface, not mark the slot as paid.
- **Weekly totals sum `activity_seconds` across slots**, never `dominant × 15`. An 8m core + 7m side slot reports both.
- **Quests and policy are versioned per slot** (`quest_versions` / `policy_versions`): edits affect only slots that have not started.
- **`document_path` and `screenshot_path` are separate evidence channels.** `slots.screenshot_path` (and the legacy `samples.path`) must never enter the Judge text haystack; `hint` reads only `app`, `title`, `url`, `document_path`; never fabricate a `document_path` from a window title.
- **Vision is fail-closed.** `call_vision_api` accepts only `SanitizedVisionContext`. A protected capture (Never Capture app or secure input) forbids the whole HTTP request including the JPEG; protected *history* is redacted but does not block an unprotected screenshot. Corrupt `capture_context_json`, invalid category, or a failed API call → `vision = None` → gray-zone `pending_review`; never `final`.
- **`capture_context()` is called only when a screenshot is actually about to happen**, in one AppleScript, next to `capture_frontmost_window` — never once per 15s sample.
- Never capture is also never *upload*; built-in entries (1Password, Bitwarden, Keychain Access) are not deletable.
- Tests must not call `std::env::set_var("HOME", …)` — they run in parallel and clobber each other.

## Specs and plans

`docs/superpowers/specs/` is the design source of truth, `docs/superpowers/plans/` holds task-by-task implementation plans (checkbox steps, each task ending in one commit). `2026-09-11-wave1-evidence-and-vision-design.md` **supersedes** specific parts of `2026-09-10-gamelife-design.md` (path split, vision context, default policy, vision JSON validation); anything it does not mention still follows the earlier design. Check the spec before implementing behavior — the plans reference its section numbers directly.
