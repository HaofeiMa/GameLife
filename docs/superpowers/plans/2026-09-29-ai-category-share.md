# Category-share gray zone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gray-zone text and screenshot calls return time shares for 主线 / 支线 / 杂项 / 娱乐, and the category with the most seconds covers the whole observed slot.

**Architecture:** Hard-rule away and entertainment seconds stay as measured. The undecided remainder is `observed - away - distraction`. The model splits only that remainder. Rounded seconds plus those hard-rule seconds pick a winner; ties follow 主线, 支线, 杂项, 娱乐, 离开. The winner then owns every observed second. `judge_slot` is still called with `vision: None`.

**Tech Stack:** Rust, `gamelife-core`, Tauri app crate, React settings copy. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-29-ai-category-share-design.md`

## Global Constraints

- Hard rules and metadata auto-settle stay before any AI call. Admin and side lists do not auto-classify. Entertainment-list hits stay a hard rule.
- Shares are only of the undecided seconds. Hard-rule entertainment is added after `round(entertainment × undecided)`, not multiplied again. Rounding is `f64::round` (half away from zero).
- A share must be finite and in `0.0..=1.0`. The four shares must sum to within `0.02` of `1.0`. Otherwise that step does not settle.
- Tie order is 主线, 支线, 杂项, 娱乐, 离开, first category whose rounded seconds equal the max. All five at 0 stays pending review.
- The winner is written as the whole `observed_seconds`. Other work buckets are cleared. Unobserved gaps stay unobserved. Pay is that same observed duration: mainline full and counted toward 8 hours; side and admin use the existing discounts and do not count; entertainment and away pay 0. Existing `min(observed, slot length, 900)` cap stays.
- Empty task list still calls AI when undecided seconds are greater than 0. Undecided `0` does not call AI.
- Protected captures are never uploaded. Protected history is redacted from the text summary and does not block an unprotected screenshot.
- Already `final`, `unknown`, or `pending_review` slots are not rejudged.
- UI strings are Chinese. Energy is **能量**.
- Tests must not call `std::env::set_var("HOME", …)`.
- App crate tests use `/opt/homebrew/bin/cargo test --offline -p gamelife`. Core tests: `/opt/homebrew/bin/cargo test --offline -p gamelife-core`. Frontend: `npx vitest run --dir src`.
- One English conventional commit per task. Do not commit unrelated dirty files.
- Do not follow `2026-09-29-ai-task-match-prompt-design.md`. It is superseded.

## File map

| File | Responsibility |
| --- | --- |
| `crates/gamelife-core/src/task_ai.rs` | Parse four shares and cover the slot. Prompt intro asks for those shares. |
| `crates/gamelife-core/src/lib.rs` | Export the new parse and settle functions. |
| `src-tauri/src/scheduler.rs` | Call the new settle path, including an empty task list. Skip AI when undecided is 0. |
| `src/pages/Settings.tsx`, `CLAUDE.md`, `AGENTS.md`, `.cursor/rules/specs.mdc` | Judgment copy and the spec index. |

---

### Task 1: Cover the slot from category shares

**Files:**
- Modify: `crates/gamelife-core/src/task_ai.rs`
- Modify: `crates/gamelife-core/src/lib.rs` (the `pub use task_ai::` list)
- Test: `crates/gamelife-core/src/task_ai.rs`

**Interfaces:**
- Consumes: `SlotEvidence.activity.away`, `SlotEvidence.activity.distraction`, `SlotEvidence.observed_seconds`, `payout_base_seconds` (that value is the undecided seconds).
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CategoryShares {
    pub mainline: f64,
    pub side: f64,
    pub admin: f64,
    pub entertainment: f64,
}

pub enum CategoryShareError {
    InvalidJson,
    BadShare,
    BadSum,
}

pub fn parse_category_shares(json: &str) -> Result<CategoryShares, CategoryShareError>

/// `shares == None` is only valid when undecided seconds are 0.
/// Returns `pending: true` when every vote is 0.
pub fn apply_category_shares(
    output: JudgeOutput,
    ev: &SlotEvidence,
    shares: Option<&CategoryShares>,
) -> JudgeOutput

pub fn settle_from_category_shares(
    output: JudgeOutput,
    ev: &SlotEvidence,
    raw: &str,
) -> Option<JudgeOutput>
```

Unknown JSON keys are ignored. Missing any of the four keys, a non-number, a non-finite number, or a number outside `0.0..=1.0` is `BadShare`. `abs(sum - 1.0) > 0.02` is `BadSum`. `settle_from_category_shares` returns `None` when parse fails or when `apply_category_shares` leaves `pending == true`.

Leave `parse_task_match_json` and `settle_from_text_ai` in the crate for this task. Task 2 stops the scheduler from calling them.

- [ ] **Step 1: Write the failing tests**

Add these in `task_ai.rs` `mod tests`. Use the existing `empty_evidence` / `empty_output` helpers. `empty_output` must stay `pending: true`.

```rust
#[test]
fn eight_minutes_mainline_covers_seven_minutes_side() {
    let ev = empty_evidence(900);
    let raw = r#"{"mainline":0.5333333333,"side":0.4666666667,"admin":0,"entertainment":0}"#;
    let out = settle_from_category_shares(empty_output(900), &ev, raw).expect("settles");
    assert_eq!(out.dominant, Dominant::CoreResearch);
    assert_eq!(out.credited_core_seconds, 900);
    assert_eq!(out.activity.core, 900);
    assert_eq!(out.activity.side, 0);
    assert!(!out.pending);
}

#[test]
fn five_minute_tie_picks_side_before_admin_and_away() {
    let mut ev = empty_evidence(900);
    ev.activity.away = 300;
    let raw = r#"{"mainline":0,"side":0.5,"admin":0.5,"entertainment":0}"#;
    let out = settle_from_category_shares(empty_output(900), &ev, raw).expect("settles");
    assert_eq!(out.dominant, Dominant::SideProject);
    assert_eq!(out.credited_side_seconds, 900);
    assert_eq!(out.activity.side, 900);
    assert_eq!(out.activity.admin, 0);
    assert_eq!(out.activity.away, 0);
}

#[test]
fn bad_sum_does_not_settle() {
    let ev = empty_evidence(900);
    let raw = r#"{"mainline":0.5,"side":0.5,"admin":0.5,"entertainment":0.5}"#;
    assert!(settle_from_category_shares(empty_output(900), &ev, raw).is_none());
}

#[test]
fn zero_votes_stay_pending() {
    let ev = empty_evidence(0);
    let out = apply_category_shares(empty_output(0), &ev, None);
    assert!(out.pending);
}
```

- [ ] **Step 2: Run the new test and confirm it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::tests::eight_minutes_mainline_covers_seven_minutes_side -- --nocapture`

Expected: FAIL to compile because `settle_from_category_shares` does not exist.

- [ ] **Step 3: Implement**

`undecided = payout_base_seconds(ev)`.

When `undecided > 0` and `shares` is `None`, return the input output unchanged with `pending` left true.

Otherwise:

```rust
let main = round_share(shares.map(|s| s.mainline).unwrap_or(0.0), undecided);
let side = round_share(shares.map(|s| s.side).unwrap_or(0.0), undecided);
let admin = round_share(shares.map(|s| s.admin).unwrap_or(0.0), undecided);
let entertainment = ev.activity.distraction
    + round_share(shares.map(|s| s.entertainment).unwrap_or(0.0), undecided);
let away = ev.activity.away;
```

`round_share(p, secs)` is `(p * secs as f64).round() as i64` when `secs > 0`, else `0`.

Walk `(主线, main)`, `(支线, side)`, `(杂项, admin)`, `(娱乐, entertainment)`, `(离开, away)`. The winner is the first whose seconds equal the maximum. If that maximum is `0`, leave `pending` true and do not rewrite buckets.

On a win, set `activity.core/side/admin/support/distraction/away` all to `0`, then write `ev.observed_seconds` into the winner's bucket. Set the matching `credited_*` to `observed_seconds` for 主线 / 支线 / 杂项, and `0` for 娱乐 / 离开. Clear the other credited fields. Set `dominant` to `CoreResearch`, `SideProject`, `Admin`, `Distraction`, or `BreakAway`. Set `pending` false. Do not change `activity.unobserved`.

- [ ] **Step 4: Run the core tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: cover a gray slot with the largest category share

EOF
)"
```

---

### Task 2: Ask for shares and settle the slot with them

**Files:**
- Modify: `crates/gamelife-core/src/task_ai.rs` (`TASK_MATCH_INTRO`)
- Modify: `src-tauri/src/text_ai.rs` tests that require the old intro
- Modify: `src-tauri/src/scheduler.rs` (`finalize_slot_end_in` around lines 1624–1664, and the gray-zone tests that post `task_id` JSON)
- Test: `crates/gamelife-core/src/task_ai.rs`, `src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: `settle_from_category_shares` and `apply_category_shares` from Task 1. `payout_base_seconds`. `compose_vision_prompt`.
- Produces: `TASK_MATCH_INTRO` is exactly:

```text
估计尚未被硬规则定性的窗口时间里，主线、支线、杂项、娱乐各占多少。四个数相加为 1。未完成任务、类别说明和应用名单只是估计依据，不是要返回的任务。离开和已经命中娱乐名单的时间不要放进这四个数。只回复 JSON {"mainline": number, "side": number, "admin": number, "entertainment": number}。
```

`finalize_slot_end_in` calls AI whenever the slot is gray and `payout_base_seconds(&evidence) > 0`, even when `prepared.shown` is empty. Text runs only when `summary` is non-empty. A legal text reply skips vision. Vision runs only when text did not settle, undecided seconds are greater than 0, capture is `Captured`, context parsed, and `prepare_screenshot_request` returned `Ok`. Both calls parse with `settle_from_category_shares`. Vision sets `used_vision = true` on the settled output. When undecided seconds are 0 and the slot is still gray, call `apply_category_shares(output, &evidence, None)` and keep it only if `pending` is false.

- [ ] **Step 1: Write the failing tests**

In `task_ai.rs`:

```rust
#[test]
fn prompt_asks_for_category_shares_not_a_task_id() {
    let built = build_task_match_prompt(
        &[],
        &CategoryGuides::default(),
        &default_v01(),
        "app=微信 title=微信 url= document_path= idle=0",
    );
    assert!(built.text.contains("四个数相加为 1"));
    assert!(built.text.contains("\"mainline\": number"));
    assert!(!built.text.contains("task_id"));
}
```

In `scheduler.rs`, replace `gray_text_chain_skips_client_and_low_confidence_until_admin_lands` so the successful body is `{"mainline":0,"side":0,"admin":1,"entertainment":0}` and the low-confidence body is no longer a confidence field. Use a body whose shares sum to `2` on the middle provider and a client error on the first. Expect `Dominant::Admin` and `credited_chore_seconds == 900`.

Replace `gray_vision_task_json_sets_used_vision_and_category_json_does_not`: a category-key-only object is no longer the failure case. Failure is `{"mainline":0.5,"side":0.5,"admin":0.5,"entertainment":0.5}`. Success is `{"mainline":0,"side":0,"admin":1,"entertainment":0}` with `used_vision` true, `Dominant::Admin`, and `activity.admin == 900`. The helper must call `settle_from_category_shares` and then set `used_vision`.

- [ ] **Step 2: Run the prompt test and confirm it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::tests::prompt_asks_for_category_shares_not_a_task_id -- --nocapture`

Expected: FAIL because the intro still mentions `task_id`.

- [ ] **Step 3: Implement**

Replace `TASK_MATCH_INTRO` with the sentence in Interfaces. Update `text_ai.rs` tests that assert the old English intro or the substring `task_id`. `prompt_with_snapshot_asks_for_task_id` should instead assert the new intro and that a snapshot line `id=` is still present when a task is passed.

In `finalize_slot_end_in`, compute `let undecided = payout_base_seconds(&evidence);` after `judge_slot`. Branch:

```rust
if gray && undecided == 0 {
    let covered = apply_category_shares(output.clone(), &evidence, None);
    if !covered.pending {
        output = covered;
    }
} else if gray && undecided > 0 && !summary.is_empty() {
    output = settle_gray_text_from_shares(output, &evidence, &chain, |ep| {
        vision::complete_json(ep, &prepared.text, None)
    });
}
if output.pending && gray && undecided > 0 {
    // existing Captured + parseable context + prepare_screenshot_request Ok path
    // prompt = compose_vision_prompt(&prepared.text, &sanitized)
    // settle via settle_from_category_shares, then used_vision = true
}
```

`settle_gray_text_from_bodies` today passes `&prepared.shown` into `settle_from_text_ai`. Change that helper, or add `settle_gray_text_from_shares`, so the parse function is `settle_from_category_shares` and does not take a task slice. Do the same for `settle_gray_vision_task`. Do not pass a `VisionResult` into `judge_slot`.

Delete scheduler tests that still expect a `task_id` to close a gray slot.

- [ ] **Step 4: Run tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::`

Expected: PASS.

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS. This is the app-crate gate.

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs src-tauri/src/text_ai.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: settle gray slots from category time shares

EOF
)"
```

---

### Task 3: Settings copy and agent docs

**Files:**
- Modify: `src/pages/Settings.tsx` (判定顺序 near the current 「必须从这些任务里点一条」 item, and the 类别说明 caption)
- Modify: `CLAUDE.md` (judgment pipeline items 3 and 4; spec index)
- Modify: `AGENTS.md` (the sentence added about naming an unfinished task)
- Modify: `.cursor/rules/specs.mdc` (spec list)

**Interfaces:**
- Consumes: the behavior from Tasks 1–2. No new Rust functions.
- Produces: the four 判定顺序 sentences and the caption below, copied verbatim.

Caption:

`灰字为样稿。空着保存不进提示。写了也只在灰区帮助 AI 估计时间比例，不直接定类别。`

判定顺序:

1. `硬规则：锁屏或暂停 → 离开；娱乐名单 → 娱乐；闲置满 3 分钟 → 离开；GameLife → 支线；未完成任务的标题对上 → 按该任务的角色。对不上 → 待定。杂项名单和支线名单不在这一步定性。`
2. `元数据够确定就直接结算，不调 AI：匹配到的主线满 13 分钟，且支线、杂项、娱乐合计不超过 1 分钟 → 主线；这三类合计满 5 分钟且压过主线 → 归到占优的一类；离开满 10 分钟且匹配主线不足 5 分钟 → 离开。`
3. `剩下的是灰区。没有尚未定性的时间：在已经量到的离开和娱乐里取较长的一类盖住整段已观测时间。有尚未定性的时间：文本判断返回主线、支线、杂项、娱乐的时间比例。任务、类别说明和应用名单只帮助估计，不必对应某条任务。`
4. `文本的比例不合法，且这一槽有可用截图：截图判断用同一份回复再估一次。取秒数最多的一类盖住整段已观测时间。整数秒相同时按主线、支线、杂项、娱乐、离开取第一个并列的。没有截图、截图受保护或回复不合法 → 待复核，不猜类别。`

`CLAUDE.md` items 3 and 4: gray text and vision return the four shares; the largest rounded share, plus hard-rule away and entertainment, covers the whole observed slot; an empty task list still calls AI when undecided seconds remain; invalid replies stay `pending_review`. In the spec index, list `2026-09-29-ai-category-share-design.md` as superseding `2026-09-29-ai-task-match-prompt-design.md`.

`AGENTS.md`: replace the sentence that AI must name an unfinished task with: unmatched gray time is estimated as category shares; the largest share covers the slot.

`.cursor/rules/specs.mdc`: add the same one-line supersession.

- [ ] **Step 1: Replace the settings strings and the three docs**

Use the verbatim sentences above. Do not change Rust.

- [ ] **Step 2: Run the frontend gate**

Run: `npx vitest run --dir src`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/pages/Settings.tsx CLAUDE.md AGENTS.md .cursor/rules/specs.mdc
git commit -m "$(cat <<'EOF'
docs: describe gray-zone AI as category time shares

EOF
)"
```

Also include the spec status line if you marked `2026-09-29-ai-task-match-prompt-design.md` superseded in this task. The new spec file `docs/superpowers/specs/2026-09-29-ai-category-share-design.md` must be part of the branch. If it is still untracked at the start of this task, add it in this commit.

---

## Self-review

Spec §2 rows map to tasks: call timing and empty tasks (Task 2), materials and reply shape (Task 2 intro plus Task 1 parser), cover and ties and pay (Task 1), vision `used_vision` (Task 2), old slots (unchanged `pending_review` early return), settings copy (Task 3). Historical `research_support` labels stay on the timeline; no task deletes them.

## Execution note

Do not start Task 1 until this plan is the one the user asked to execute. Task 2 does not compile against the new settle functions until Task 1 has landed.
