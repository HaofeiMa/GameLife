# CLAUDE.md

This file provides guidance to coding agents working in this repository (Claude Code, Cursor, and others). Cursor also has a shorter operational index in `AGENTS.md`.

## Project

GameLife — a macOS tray app (Tauri 2 + React + TypeScript + Rust). It samples the frontmost window every 15 seconds, classifies each 15-minute slot (mainline / side / admin / entertainment / away / unobserved), and pays coins + energy for credited mainline seconds toward a 480-minute workday. It is an **activity monitor, not a planner**: scheduling lives in TickTick (read-only Open API); this app observes, judges, pays, reports.

Runs tray-only: closing the window hides it, only tray → 退出 exits the process. UI strings and all specs/plans are Chinese; commit messages are English conventional commits (`feat:`, `fix:`), one per plan task.

Data root: `~/Library/Application Support/GameLife/` — `gamelife.db`, `config.json`, `secrets.json`, optional `screenshots/`. Secrets (API keys for each vision provider, TickTick access/refresh token and client secret) live in `secrets.json`, mode 0600, and never in `config.json`. That file is what `src-tauri/src/keychain.rs` reads and writes; the module keeps its historical name but the macOS Keychain is no longer used.

## Commands

```bash
npm install
npm run tauri dev            # run the app (tray + window)
npm run tauri build          # release bundle
npm run build                # tsc + vite build — frontend typecheck

cargo test --offline -p gamelife-core     # pure domain logic, no DB/OS (~1s)
cargo test --offline -p gamelife          # DB, scheduler, macOS, TickTick, AI (~25s)
npx vitest run --dir src                  # frontend (~1s)

# single test / module filter
cargo test --offline -p gamelife-core judge::
cargo test --offline -p gamelife scheduler::tests::tick_capture_skips_never_capture_app
npx vitest run --dir src lib/format
```

The gates every spec repeats, and that must hold before a plan task is committed:

- Any change under `src-tauri/` — `cargo test --offline -p gamelife` must **run**, not just compile. `--no-run` is not sufficient.
- Any change under `crates/gamelife-core/` — `cargo test --offline -p gamelife-core`.
- Any change under `src/` — `npx vitest run --dir src`.
- Any change under `src-tauri/src/{windows,linux,observe}/` or `tools/platform-check/` — from `tools/platform-check`, rustup toolchain on `PATH`, own `CARGO_TARGET_DIR`:
  `cargo check --target x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`. Never from the repo root. Update `contract()` if a backend gains a method.

The cross-platform backends under `src-tauri/src/{windows,linux}/` cannot be compiled as part of this crate — `rusqlite` is `bundled`, so a target C toolchain is needed. They are written to depend on nothing but `std` / `gamelife-core` / the platform crate, and `tools/platform-check/` (excluded from the workspace) `#[path]`-includes the real files so they can be checked:

```bash
cd tools/platform-check
TC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-pc-windows-msvc
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-unknown-linux-gnu
```

The toolchain must be explicit (`PATH`) and the target dir must be its own; never run this from the repo root, where it would try to build the whole app and mix two compilers into `target/`. Its `contract()` calls every method the sampler calls with the same signatures — that is the only thing that proves a backend still satisfies what the app asks of it, so update it when a backend gains a capability. `HANDOFF.md` has the rest of the cross-platform state.

`--offline` is the README's convention (all deps are in `Cargo.lock` + the local registry cache); drop it only if you actually need to fetch. Unscoped `npx vitest run` also collects the test copies inside `.worktrees/` and inflates the counts, so always pass `--dir src`.

The checkout lives on an external volume at `/Volumes/MobileSSD/Program/My/GameLife`. It was moved here from `/Users/mahaofei/Projects/GameLife`, which left `target/` holding build-script output keyed to the dead path and made the `gamelife` build script fail with a `permission files` error. Deleting `target/debug/build/*` cleared it; if it recurs after another move, do the same.

## Architecture

A two-crate Cargo workspace with a hard layering rule:

- **`crates/gamelife-core`** — pure domain logic, no I/O. Dependencies are only `chrono`/`serde`/`serde_json`: no `rusqlite`, no `tauri`, no filesystem. Holds classification (`hint`), span arithmetic (`observe`), the slot decision (`judge`), reward keys (`ledger`), `policy`, `vision_ctx`, `quest`, the TickTick side (`task`, `task_parse`, `task_ai`), and the report aggregates (`app_stats`, `reports`, `feel`, `streak`, `shop`, `early_start`, `document`, `url`). Everything here is unit-testable in-process; keep it that way.
- **`src-tauri`** — the app shell. `db.rs` (schema + `PRAGMA user_version` migrations, target `user_version = 3`), `observe/` (platform seam: `imp` is `macos/` / `windows/` / `linux/`), `sampler.rs` (the 15s loop behind a `SampleSource` trait so tests inject a fake), `scheduler.rs` (~3.7k lines: slot lifecycle, random capture scheduling, policy seeding, gray-zone AI, finalize, settle, midnight, TickTick cache refresh), `resolve.rs` (one transaction writing `JudgeOutput` into `slots` + `ledger` + the daily app/host rollups), `commands.rs` (the 38 Tauri commands), `vision.rs` / `text_ai.rs` (HTTP to OpenAI-compatible providers), `ticktick.rs` (OAuth PKCE + read-only sync), `config.rs` (`config.json`), `keychain.rs` (`secrets.json`), `platform.rs` (the per-OS data dir and the default-browser opener — the only place a platform convention is decided).

`macos/` is in-process native API, not shelling out, with one exception:

- `snapshot.rs` / `mod.rs` — Accessibility metadata (app, title, bundle id, `AXDocument`) behind a cached `ObservationState`; `capture_context()` and `document_path()` read from the same cache.
- `browser.rs` — the **only** remaining `osascript`: the current tab URL for Chrome / Safari / Arc.
- `capture.rs` — ScreenCaptureKit JPEG of the frontmost window, keyed by `CGWindowID` (`window_id.rs`).
- `input.rs` — CoreGraphics idle / lock / secure-input plus the accessibility and screen-recording permission booleans.

Data flow: sampler tick → `samples` row + `heartbeat`; `ensure_slot` pins the policy version, quest version, and TickTick task snapshot for the slot; `tick_capture` takes one screenshot at a random 4–13 min offset (persisted as `capture_scheduled_at`); at slot end `finalize_slot_end` → hints → `judge_slot` (core) → `resolve_slot` (ledger).

`get_week(anchor?)` reports any week, not just the current one: the anchor picks the Mon–Sun range, and a week that is still running stops at today so a future day is never counted as missed. Its wallet fields (coin balance, XP today, live session, shop unlock) are always **now** regardless of the anchor — 统计 and 商店 share the command, and only 统计 pages it.

**The judgment pipeline** (`scheduler::finalize_slot_end` + `core::judge`) is the heart of the app and spans several files. In order:

1. **Hard rules per sample** (`hint.rs`): locked/paused → `Away`; distraction rules → `Distraction`; idle ≥ 180s → `Away`; admin rules → `Admin`; side rules → `Side`; Core candidate → `CoreCandidate`; otherwise `Unsure`. **There is no app whitelist** — `trusted_apps` no longer gates candidacy (any app showing a work file or a title carrying a mainline evidence token qualifies); the entertainment / admin / side rules, which all read the app name, are what keep a window out. The idle gate sits between distraction and admin on purpose — a video left playing stays entertainment, while an unattended 杂项 or 支线 window becomes absence instead of earning. Distraction, admin and side all scan the whole haystack (app / title / url / document_path); admin alone also tries app identity first, so the known-bundle table still resolves localized names. Observation beats plan — a slot whose samples hit an entertainment rule is entertainment and pays 0 even if TickTick scheduled mainline then.
2. **Decidable from metadata** (`metadata_decidable`): grounded strong core ≥ 13 min auto-cores, dominant distraction auto-distracts — no AI call at all.
3. **Slot-end text AI** (`text_ai.rs`) for what remains gray: with a pinned TickTick snapshot it does a task match, without one a category match against the four one-line `category_guides`.
4. **Vision fallback** (`vision.rs`) on the slot's screenshot, primary provider then fallback, when the text pass did not resolve the slot.

Frontend: `src/pages/*` is one file per tab (`Today.tsx` / `Stats.tsx` / `Shop.tsx` / `Settings.tsx`), `src/lib/api.ts` is the single place that calls `invoke()`, and the rest of `src/lib/*` holds pure view-model helpers with colocated `*.test.ts`. `src/lib/format.ts` mirrors core formatting.

### Frontend UI

Tailwind v4 (CSS-first, via `@tailwindcss/vite` — no `tailwind.config.*`, no PostCSS) over a warm 存档 (save-file) token set taken from the UI mockup — not a stock shadcn theme. Light and dark are both driven by the tokens in `src/index.css`; nothing hardcodes a colour.

- **`src/index.css`** — the design tokens (`:root` / `.dark` as bare HSL triplets consumed via `hsl(var(--x))`), the `@theme inline` mapping that turns them into utilities, and the base layer. `inline` matters: it emits `hsl(var(--background))` rather than `var(--color-background)`, which is what lets the `.dark` overrides win. Category colours live here as `--cat-*` and are exposed as `bg-mainline` / `text-side` / …. Every light token is written `triplet; /* #HEX */` and the mockup's hex is the source of truth: `src/lib/palette.test.ts` converts each triplet back to 8-bit and asserts it equals the comment, so **trimming the decimals breaks the palette** (an integer triplet is a full channel step off) and editing one half without the other fails the test. Dark is asserted separately as the warm charcoal `#1C1917`, never a cool grey.
- **`src/lib/theme.ts`** — the only place TS may name a colour: `categoryColor` / `categoryColorAt` return CSS-variable references (so inline styles follow the theme), and `categoryOf` maps an engine role onto a category key. Do not write a hex value in a component.
- **`src/hooks/useTheme.ts`** + the pre-paint script in `index.html` — applies `theme` from config, following the system while it is `system`. The script caches the resolved theme in `localStorage` so an explicit override does not flash the system default on launch.
- **`src/components/ui/*`** — the primitives (button, card, input, select, switch, segmented, dialog, progress, skeleton, empty-state, stat-tile, toaster). Hand-rolled, no Radix: the app has three dialogs and no nested ones, so `Dialog` is a focus trap plus Esc plus a scroll lock.
- **`src/App.tsx`** imports the sidebar logo straight from `src-tauri/icons/128x128@2x.png` (Vite inlines it), so the in-app mark and the dock icon cannot drift apart.
- **`src/hooks/useSidebarWidth.ts`** — the divider between the sidebar and the content is draggable (pointer capture + arrow keys). The width is window geometry, not a product setting, so it lives in `localStorage` rather than `config.json`.
- **`src/components/PageHeader.tsx`** — each page returns `<>{header}<scroll area/></>`. The header owns its own vertical space rather than sticking, and is a window drag region. macOS runs `titleBarStyle: "Overlay"`, so the traffic lights sit over the top-left of both columns. `TRAFFIC_LIGHT_STRIP` = 28, `TOOLBAR_ROW` = 58 (the mockup's `.hd`), `BRAND_ROW` = 54 (its `.brand`, deliberately 4px shorter), `TOOLBAR_PAD_BOTTOM` = 0, and `TOOLBAR_HEIGHT` is their sum — the design's own heights are preserved by paying for the traffic lights once, in a strip *above* both columns. `App.tsx` sizes the sidebar's brand block from `TRAFFIC_LIGHT_STRIP` + `BRAND_ROW`; drift those apart and the first nav item rides above the header's bottom border. The title is followed by a hairline and `subtitle`, then `center` (`ml-4`, 16px behind the title block — it follows the title, it is not centered in the bar), then `actions` pulled right with `ml-auto`. Everything interactive inside is `no-drag` while the bar itself is a drag region.
- **Drag regions are an ACL concern, not a CSS one.** `data-tauri-drag-region` with no value only drags on a direct hit on that very element (`el === composedPath[0]`), so a bar full of children needs `="deep"`; and the injected script's `plugin:window|start_dragging` is denied unless `src-tauri/capabilities/default.json` grants `core:window:allow-start-dragging` — `core:window:default` is all read-only. `-webkit-app-region` is inert in this Tauri version and hides both requirements. `IS_MACOS` (`src/lib/platform.ts`) gates the traffic-light strip, which is 0 off macOS.
- Rail tabs live in `src/lib/rail.ts`. The statistics tab's **id is `week`** (legacy); the label is 统计 and the page is `src/pages/Stats.tsx`.
- Convention: blue = primary action, green = enabled/healthy (switches are green when on), amber = warning, red = destructive only. Page gutter `px-6`, cards `rounded-xl border bg-card`, focus is the global `*:focus-visible` outline.

## Invariants that are easy to break

These come from the specs and are enforced by tests — read the relevant spec section before changing judgement, capture, ledger, or reporting code.

- **No extrapolation across gaps.** Adjacent samples more than `2 × 15s` apart leave the middle `unobserved`; never fill it from the endpoints. Always `credited ≤ observed ≤ actual slot duration`.
- **Process death is `unobserved`, never `away`.** Exit / crash / force-quit / reboot gaps become `unobserved` with credited 0. Pause while the process lives is `break_away`. A cross-midnight restart only backfills to 24:00 of the heartbeat's own day. A missing Accessibility or Screen-Recording permission is the same shape — `unobserved` / `missing` with the orange banner, never `away` and never a paid slot. **Long idle (≥ `LOW_INPUT_IDLE_SECS`, 180s) IS itself `away`**, checked after the distraction rule and before admin / side / trusted — a playing video is still entertainment, but an unattended 杂项 or 支线 window is absence. `reading_apps` and the `CoreReading` reading bridge are therefore inert — the away gate claims every sample the bridge could have fired on. `reading_bridge_seconds` is always 0.
- **The macOS data directory must not move.** `platform.rs` resolves `~/Library/Application Support/GameLife` on macOS, `%APPDATA%\GameLife` on Windows and `$XDG_DATA_HOME/GameLife` (else `~/.local/share/GameLife`) on Linux from one pure, unit-tested function. Repointing the macOS branch orphans every existing database, `config.json` and `secrets.json`.
- **Linux observes only an Xorg session.** Wayland has no "which window is focused" query; `observation_status` reports `supported=false`. Windows and X11 backends exist; they have not been run on real machines. Identity aliases (`chrome` → `Google Chrome`, `Code` → `Visual Studio Code`) live in `known_app_identities` and match exact stems/classes, not substrings. Don't let Wayland look merely idle.

- **Final/unknown slots are immutable.** Reporting a misclassification writes a record only — no retroactive economic correction.
- **Ledger writes are idempotent** via `UNIQUE reward_event_key`. Only `DbOpError::AlreadyApplied` may be swallowed; IO/FULL/CORRUPT must surface, not mark the slot as paid.
- **Reporting totals sum `activity_seconds`** across slots (or `SUM` the daily rollup tables), never `dominant × 15`. An 8m core + 7m side slot reports both.
- **Daily rollups are written in the same transaction as the slot decision** (`app_day_stats`, `host_day_stats` in `resolve.rs`). A slot must never be `final` with its rollup missing. The rollup is what keeps month statistics alive after `sample_keep_days` deletes the raw samples.
- **Quests and policy are versioned per slot** (`quest_versions` / `policy_versions`): edits affect only slots that have not started. Quests, local `tasks` / `task_lists` and their commands are legacy — still in the schema and still readable by `judge_slot`, but no UI calls them.
- **`document_path` and `screenshot_path` are separate evidence channels.** `slots.screenshot_path` (and the legacy `samples.path`) must never enter a Judge or AI text haystack; `hint` reads only `app`, `title`, `url`, `document_path`; never fabricate a `document_path` from a window title.
- **Vision is fail-closed.** `call_vision_api` accepts only `SanitizedVisionContext`. A protected capture (Never Capture app or secure input) forbids the whole HTTP request including the JPEG; protected *history* is redacted from the text summary but does not block an unprotected screenshot. Corrupt `capture_context_json`, invalid category, or a failed API call → `vision = None` → gray-zone `pending_review`; never `final`.
- **A protected window is never captured, never uploaded, and never paid.** Its title, path and URL are stripped from the text-AI summary too. Built-in entries (1Password, Bitwarden, Keychain Access) are not deletable.
- **`capture_context()` is called only when a screenshot is actually about to happen** — once, inside `tick_capture_impl`, immediately before `capture_window`, and never once per 15s sample (`tick_capture_before_schedule_does_not_call_capture_context`).
- **Commands that touch the network must be `async`.** Tauri runs synchronous commands on the main thread, so a blocking HTTP call in one freezes the webview for its whole duration — this is what made opening 设置 → TickTick hang for seconds. The async commands are `ticktick_tree` (only when `refresh: true`), `ticktick_sync`, `ticktick_begin_oauth` / `ticktick_finish_oauth`, and `test_vision_provider`. Opening 设置 must read the cached tree (`refresh` omitted/false); only 同步任务 / an explicit refresh hits the network.
- **TickTick roles: a column beats its project.** `ticktick_project_roles` maps a project id to a role; `ticktick_column_roles` maps `"{projectId}:{columnId}"` to one, so two projects can each have a "Today" column without colliding, and the backend can work out which projects to sync without first fetching every project's columns. There is no longer a project-vs-column mode flag — both maps apply at once, and an unmarked column falls back to its project's role. Roles are not part of the policy snapshot, so changing one does not cut a new policy version.
- **The TickTick tree is cached in `app_meta` under `ticktick_tree`**, written by `sync_projects` from the same `/project` + `/project/{id}/data` fetches it already makes. 设置 reads the cache so opening the tab is instant; only 同步任务 hits the network.
- **TickTick is read-only and one-way.** OAuth tokens and snapshots come in; screenshots, window titles, and reports never go out. Judgement must not depend on TickTick being connected — with no snapshot, a grounded work path still auto-cores (`judge.rs::empty_quests_work_path_still_auto_cores`). No `ticktick-cli` at runtime.
- **UI is Chinese, and energy is called 能量** — never XP / 经验 in user-facing text, even though the ledger column stays `xp_delta`. No FullCalendar, no dnd-kit, no local task CRUD UI, no self-reported timing.
- **Copy nothing from a GPL project.** The reward feel is inspired by Habitica, which is GPL — read it for ideas, never copy its source or assets. The shell's provenance is cc-switch (MIT), which is safe to borrow from.
- **Category colour has exactly one source** (`src/index.css` `--cat-*`, mirrored for inline styles by `src/lib/theme.ts`). It used to be written out three times — in CSS, in the page component arrays, and inline in chart components — and drifted. Do not reintroduce a literal colour in a component.
- **Every page must keep working without TickTick and with the window closed.** Toasts are the only reward feedback (no system notifications, no sound); missing them is explicitly not a failure.
- Tests must not call `std::env::set_var("HOME", …)` — they run in parallel and clobber each other.

## Specs and plans

`docs/superpowers/specs/` is the design source of truth, `docs/superpowers/plans/` holds task-by-task implementation plans (checkbox steps, each task ending in one commit). Check the spec before implementing behavior — the plans reference its section numbers directly.

Later specs override earlier ones only where they say so; read the "本文覆盖并取代" block at the top of each before assuming a rule is current:

- `2026-09-10-gamelife-design.md` — the base product spec. Still authoritative on the economy (coin/XP ticks, Gold Day, chest), gaps, and ledger idempotency.
- `2026-09-11-wave1-evidence-and-vision-design.md` — supersedes it on the path split, capture context, default policy, and vision JSON validation.
- `2026-09-11-observation-engine-design.md` — supersedes Wave 1's "osascript is fine"; this is where ScreenCaptureKit and the native `macos/` module come from.
- `2026-09-11-judgment-grounded-core-design.md` — supersedes the idle-based `away` rule and the metadata-only auto-Core threshold (`grounded_strong_core`, not `strong_core`). The idle-based `away` rule was later reinstated at 180s with a different placement (after side, before the trusted gate) on the user's call; that spec's `reading_bridge` is now dead as a result.
- `2026-09-11-today-mainline-design.md` — the Quest contract as a 1–3 item day snapshot with a hero mainline, explicit evidence, and a live hit indicator on 今日. Its Quest contract and 今日 presentation are then superseded by planner-shell and activity-monitor, so mostly read it for history.
- `2026-09-11-game-feel-design.md` — the reward feel (F3) and shop usability. It supersedes 2026-09-10 §8–10 on *presentation and shop interaction only*, leaving the economic invariants alone, and fixes the product boundary: no check-in / HP loss / levels / loot.
- `2026-09-11-planner-shell-design.md` — supersedes the Quest contract and page presentation; `2026-09-13-activity-monitor-design.md` then supersedes its planner-style 今日 page, its "no task today ⇒ credited = 0" rule, and the use of the local `tasks` table as the judgment set. The economy rules it introduced (discounts, slot-pinned snapshots, text AI → vision fallback, four-page rail) still stand.
- `2026-09-13-activity-monitor-design.md`, `2026-09-13-analytics-design.md`, `2026-09-13-shop-desire-design.md` — one delivery in three parts (judgment + TickTick + shell / statistics page / shop visual). They are marked 待用户审阅 but are what `main` already implements.
- `2026-09-14-cross-platform-observation-design.md` — Windows and Linux **Xorg** observation. Overrides 2026-09-10 §2's "no Windows". Identity aliases for `chrome` / `Code` / `WM_CLASS` are in `known_app_identities`. Wayland is out of scope.

## Stale documents

`HANDOFF.md` in the repo root is the **current** handoff (2026-09-14): the platform batches are committed; remaining dirty files are the UI / hint redesign. The README's 钥匙串 section still describes the macOS Keychain; secrets actually live in `secrets.json`.
