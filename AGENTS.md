# AGENTS.md

GameLife is a macOS **activity monitor**, not a planner. It samples the frontmost window every 15 seconds, classifies each 15-minute slot, and pays coins + 能量 for credited mainline seconds toward a 480-minute workday. Scheduling lives in TickTick (read-only); this app observes, judges, pays, reports.

Architecture, the judgment pipeline, and the easy-to-break invariants are in [`CLAUDE.md`](CLAUDE.md). Design specs are in `docs/superpowers/specs/`. Later specs override earlier ones only where they say so — read the 「本文覆盖并取代」 block first.

## Layout

| Path | What |
| --- | --- |
| `crates/gamelife-core/` | Pure domain. No I/O. Deps: `chrono` / `serde` / `serde_json` only. |
| `src-tauri/` | App shell: SQLite, sampler, `macos/`, scheduler, vision/text AI, TickTick, Tauri commands. |
| `src/` | React UI. One page per rail tab: `Today.tsx` / `Stats.tsx` / `Shop.tsx` / `Settings.tsx`. |
| `docs/superpowers/specs/` | Design source of truth. |
| `docs/superpowers/plans/` | Task-by-task plans; one English conventional commit per task. |

Data root: `~/Library/Application Support/GameLife/` (`gamelife.db`, `config.json`, `secrets.json` mode 0600, optional `screenshots/`). Secrets never go in `config.json`. `src-tauri/src/keychain.rs` reads/writes `secrets.json`; the macOS Keychain is not used.

## Commands

```bash
npm install
npm run tauri dev              # tray + window
npm run tauri build             # release bundle
npm run build                  # tsc + vite (frontend typecheck)

cargo test --offline -p gamelife-core    # domain, ~1s
cargo test --offline -p gamelife         # DB, scheduler, macOS, TickTick, AI; must *run*, not --no-run
npx vitest run --dir src                  # frontend; always pass --dir src (unscoped also collects .worktrees/)
```

Gates before a plan-task commit: `src-tauri/` → `gamelife` tests run; `crates/gamelife-core/` → `gamelife-core` tests; `src/` → vitest `--dir src`.

If `gamelife` build scripts fail with a `permission files` error after moving the checkout, delete `target/debug/build/*` (stale paths from `/Users/mahaofei/Projects/GameLife`).

## Product rules agents break

- UI strings are Chinese. Energy is **能量**, never XP / 经验 in the UI (`xp_delta` stays the ledger column).
- No FullCalendar, dnd-kit, local task CRUD, or self-reported timing.
- Colour has one source: `src/index.css` `--cat-*`, mirrored by `src/lib/theme.ts`. No hex in components.
- `invoke()` only from `src/lib/api.ts`.
- Observation beats plan. A protected window is never captured, uploaded, or paid.
- TickTick is read-only and one-way. Judgement must work with no snapshot.
- Tests must not call `std::env::set_var("HOME", …)` (parallel tests clobber each other).
- Closing the window hides it; only tray → 退出 exits. Every page must work with the window closed and without TickTick.

File-specific reminders live in `.cursor/rules/` (`core-layer`, `tauri-shell`, `frontend-ui`, `specs`).
