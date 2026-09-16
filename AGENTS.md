# AGENTS.md

GameLife is a macOS **activity monitor**, not a planner. It samples the frontmost window every 15 seconds, classifies each 15-minute slot, and pays coins + 能量 for credited mainline seconds toward a 480-minute workday. Scheduling lives on the local **任务** page; this app observes, judges, pays, reports. Optional cloud backup (WebDAV / S3) copies a trimmed snapshot; the live database stays the only judgment input.

Architecture, the judgment pipeline, cloud sync / settlement, and the easy-to-break invariants are in [`CLAUDE.md`](CLAUDE.md). Design specs are in `docs/superpowers/specs/`. Later specs override earlier ones only where they say so — read the 「本文覆盖并取代」 block first.

## Layout

| Path | What |
| --- | --- |
| `crates/gamelife-core/` | Pure domain. No I/O. Deps: `chrono` / `serde` / `serde_json` only. |
| `src-tauri/` | App shell: SQLite, sampler, `macos/` / `windows/` / `linux/`, scheduler, `sync.rs`, `settle.rs`, vision/text AI, Tauri commands. |
| `src/` | React UI. One page per rail tab: `Today.tsx` / `Tasks.tsx` / `Stats.tsx` / `Shop.tsx` / `Settings.tsx`. |
| `tools/platform-check/` | Cross-target `cargo check` for Windows / X11 backends. Not a workspace member. |
| `docs/superpowers/specs/` | Design source of truth. |
| `docs/superpowers/plans/` | Task-by-task plans; one English conventional commit per task. |

Data root: `~/Library/Application Support/GameLife/` (`gamelife.db`, optional `merged.db`, `config.json`, `secrets.json` mode 0600, optional `screenshots/`). Secrets never go in `config.json`. `src-tauri/src/keychain.rs` reads/writes `secrets.json`; the macOS Keychain is not used.

Build the app with **Homebrew** `/opt/homebrew/bin/cargo`. rustup is only for `tools/platform-check`; never share `target/` between the two.

## Commands

```bash
npm install
npm run tauri dev              # tray + window
npm run tauri build             # release bundle
npm run build                  # tsc + vite (frontend typecheck)

cargo test --offline -p gamelife-core    # domain, ~1s
cargo test --offline -p gamelife         # DB, scheduler, sync, settle, macOS, AI; must *run*, not --no-run
npx vitest run --dir src                  # frontend; always pass --dir src (unscoped also collects .worktrees/)
```

Gates before a plan-task commit: `src-tauri/` → `gamelife` tests run; `crates/gamelife-core/` → `gamelife-core` tests; `src/` → vitest `--dir src`. Cross-target check: `cd tools/platform-check` with rustup on `PATH` and `CARGO_TARGET_DIR=/tmp/pcheck-target` — never from the repo root.

If `gamelife` build scripts fail with a `permission files` error after moving the checkout, delete `target/debug/build/*` (stale paths from earlier locations).

## Product rules agents break

- UI strings are Chinese. Energy is **能量**, never XP / 经验 in the UI (`xp_delta` stays the ledger column).
- No FullCalendar, dnd-kit, Radix, or self-reported timing. Local tasks are created on 任务, not on 今日. The Today plan column reuses the same context menu, edge-resize, and copy as the Tasks calendar; changing day still goes through the date dialog.
- Colour has one source: `src/index.css` `--cat-*`, mirrored by `src/lib/theme.ts`. No hex in components.
- Pages call `invoke()` only from `src/lib/api.ts` (via `src/lib/invoke.ts`). Do not import `@tauri-apps/api` from a page.
- Observation beats plan. Lock / away / entertainment hard rules still win. A protected window is never captured, uploaded, or paid.
- Plan lives on the local 任务 page. Slot snapshots pin **all unfinished** tasks (schedule is not a gate). Auto-Core needs a matched mainline span ≥ 13 min; unmatched work goes to AI, never auto-Core from a work path alone. Repeat is complete-then-spawn. Optional OS banners: Settings 「任务通知」 (default off). `user_version` is 4 (`sort` / `repeat` / `remind_json`).
- Default sync scope `aggregate` must not upload `samples`, screenshots, or `secrets.json`. `VACUUM INTO` copies everything; delete `NEVER_SYNCED_TABLES` (`heartbeat`, `ticktick_cache`) from the **copy**. `task_lists` / `tasks` stay in the snapshot.
- `Deferred` settlement recomputes the day from `merged.db`; do not replay per-device reward events.
- Tests must not call `std::env::set_var("HOME", …)` (parallel tests clobber each other).
- Closing the window hides it; only tray → 退出 exits. Every page must work with the window closed, with an empty task board, and with sync disabled.

Not yet implemented: cloud-sync plan **T15** (统计 / 商店 read `merged.db`; 今日 pending-settlement preview) and **T16** (设置 device list / grace / last-seen). Windows and Linux Xorg backends have not been run on real machines.

File-specific reminders live in `.cursor/rules/` (`core-layer`, `tauri-shell`, `frontend-ui`, `cloud-sync`, `specs`).
