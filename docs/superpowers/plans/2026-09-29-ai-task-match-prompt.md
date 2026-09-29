# AI task-match prompt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gray-zone text and screenshot judgment both see unfinished tasks, category guides, and app lists, and both must name one of those tasks or the slot stays pending review.

**Architecture:** Settlement stays in `apply_task_match`. A core helper builds one task-match prompt and the exact task slice it showed. The scheduler sends that prompt to text, and the same slice plus the existing screenshot context to vision. Category JSON no longer settles a slot. `judge_slot` is still called with `vision: None`.

**Tech Stack:** Rust, `gamelife-core`, Tauri app crate, React settings copy. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-29-ai-task-match-prompt-design.md`

## Global Constraints

- Hard rules and metadata auto-settle stay before any AI call. Admin and side lists do not auto-classify. Entertainment-list hits stay a hard rule.
- Confidence threshold stays `0.7` (`TASK_MATCH_MIN`). Prompt cap stays 20 (`MAX_JUDGMENT_TASKS`). Local title matching still uses the full pinned snapshot.
- Pay rates do not change. Mainline is full and counts toward the 8-hour day. Side, longterm, and custom stay on the side discount. Chore stays on the chore discount.
- Protected captures are never uploaded. Protected history is redacted from the text summary and does not block an unprotected screenshot.
- Already `final`, `unknown`, or `pending_review` slots are not rejudged. Startup catch-up only finalizes slots whose `status` is still NULL and whose end is in the past.
- UI strings are Chinese. Energy is **能量**.
- Tests must not call `std::env::set_var("HOME", …)`.
- App crate tests use Homebrew cargo: `/opt/homebrew/bin/cargo test --offline -p gamelife`. Core tests: `/opt/homebrew/bin/cargo test --offline -p gamelife-core`. Frontend: `npx vitest run --dir src`.
- One English conventional commit per task (`feat:` / `fix:`). Do not commit unrelated dirty files.

## File map

| File | Responsibility |
| --- | --- |
| `crates/gamelife-core/src/task_ai.rs` | Task-match settlement, prompt body, shown-task slice. Delete the empty-snapshot category path. |
| `crates/gamelife-core/src/lib.rs` | Export the new prompt type. Stop exporting category-match types. |
| `crates/gamelife-core/src/vision_ctx.rs` | Screenshot prompt = task-match body + existing screen context. No category list. |
| `src-tauri/src/text_ai.rs` | Delegate the text prompt to the core helper. |
| `src-tauri/src/scheduler.rs` | Gray-zone calls, `used_vision`, skip empty tasks, do not re-enter `pending_review`. |
| `src-tauri/src/vision.rs` | Slot-end vision HTTP returns the raw model string. Callers parse `task_id`. |
| `src/pages/Settings.tsx` | 判定顺序 and the category-guide caption. |
| `CLAUDE.md`, `AGENTS.md` | One judgment-pipeline sentence each. |

---

### Task 1: Task match is the only AI settlement

**Files:**
- Modify: `crates/gamelife-core/src/task_ai.rs`
- Modify: `crates/gamelife-core/src/lib.rs` (exports around lines 84–87)
- Modify: `src-tauri/src/scheduler.rs` (test `gray_text_chain_skips_client_and_low_confidence_until_admin_lands`, about line 2256)
- Test: `crates/gamelife-core/src/task_ai.rs` (`mod tests`)

**Interfaces:**
- Consumes: existing `TaskMatch`, `TaskSnapshot`, `ListRole`, `payout_base_seconds`, `parse_task_match_json`.
- Produces: `apply_task_match` writes `activity.side` for side / longterm / custom and `activity.admin` for chore. `settle_from_text_ai` always parses `task_id` against the slice it is given. Empty slice, category JSON, `null`, confidence `< 0.7`, and unknown ids return `None`. `CategoryMatch`, `CategoryMatchError`, `parse_category_match_json`, and `apply_category_match` are removed.

- [ ] **Step 1: Write the failing tests**

Add these next to the existing `apply_mainline_sets_core` test in `task_ai.rs`. Delete `category_null_is_none`, `core_without_strong_core_is_pending`, `support_does_not_pay`, `category_side_admin_distraction_fill_activity_buckets`, `text_ai_low_confidence_or_null_does_not_settle`, `text_ai_confident_admin_settles_empty_board`, and `text_ai_core_without_strong_core_does_not_settle` in the same edit once the new tests are in place. Keep `text_ai_null_task_does_not_settle_even_at_high_confidence`.

```rust
#[test]
fn apply_side_and_chore_fill_activity_buckets() {
    let ev = empty_evidence(900);
    let side = apply_task_match(
        empty_output(900),
        &ev,
        &TaskMatch {
            task_id: "s".into(),
            confidence: 0.9,
            role: ListRole::Side,
        },
    );
    assert_eq!(side.dominant, Dominant::SideProject);
    assert_eq!(side.credited_side_seconds, 900);
    assert_eq!(side.activity.side, 900);
    assert!(!side.pending);
    for role in [ListRole::Longterm, ListRole::Custom] {
        let out = apply_task_match(
            empty_output(900),
            &ev,
            &TaskMatch {
                task_id: "x".into(),
                confidence: 0.9,
                role,
            },
        );
        assert_eq!(out.dominant, Dominant::SideProject);
        assert_eq!(out.activity.side, 900);
        assert_eq!(out.credited_side_seconds, 900);
    }

    let chore = apply_task_match(
        empty_output(900),
        &ev,
        &TaskMatch {
            task_id: "c".into(),
            confidence: 0.9,
            role: ListRole::Chore,
        },
    );
    assert_eq!(chore.dominant, Dominant::Admin);
    assert_eq!(chore.credited_chore_seconds, 900);
    assert_eq!(chore.activity.admin, 900);
    assert!(!chore.pending);
}

#[test]
fn apply_task_match_with_zero_payable_seconds_still_closes() {
    let mut ev = empty_evidence(900);
    ev.activity.away = 900;
    let out = apply_task_match(
        empty_output(900),
        &ev,
        &TaskMatch {
            task_id: "c".into(),
            confidence: 0.9,
            role: ListRole::Chore,
        },
    );
    assert!(!out.pending);
    assert_eq!(out.dominant, Dominant::Admin);
    assert_eq!(out.credited_chore_seconds, 0);
    assert_eq!(out.activity.admin, 0);
}

#[test]
fn category_json_does_not_settle_even_when_no_tasks() {
    let ev = empty_evidence(900);
    assert!(settle_from_text_ai(
        empty_output(900),
        &ev,
        &[],
        r#"{"category":"admin","confidence":0.9}"#,
    )
    .is_none());
}
```

In `scheduler.rs`, change `gray_text_chain_skips_client_and_low_confidence_until_admin_lands` so the task slice is one chore snapshot `id=chore` and the successful body is `{"task_id":"chore","confidence":0.9}`. The low-confidence body stays a task id at `0.4`. The client error on `a.test` stays. Expected dominant remains `Admin`.

- [ ] **Step 2: Run the new core tests and confirm they fail**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::tests::apply_side_and_chore_fill_activity_buckets -- --nocapture`

Expected: FAIL because `activity.side` is still 0, or the category test still settles.

- [ ] **Step 3: Implement**

In `apply_task_match`, after setting `credited_side_seconds` for `Side | Longterm | Custom`, set `output.activity.side = output.activity.side.max(base)`. After setting `credited_chore_seconds` for `Chore`, set `output.activity.admin = output.activity.admin.max(base)`.

Replace the body of `settle_from_text_ai` with:

```rust
let matched = parse_task_match_json(raw, tasks).ok()??;
let next = apply_task_match(output, ev, &matched);
if next.pending { None } else { Some(next) }
```

Delete `CategoryMatch`, `CategoryMatchError`, `parse_category_match_json`, and `apply_category_match`. Remove them from `crates/gamelife-core/src/lib.rs` `pub use task_ai::...`. Fix every compile error that named those types; the scheduler test from step 1 is the intended caller change.

- [ ] **Step 4: Run tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::`

Expected: PASS.

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife scheduler::tests::gray_text_chain_skips_client_and_low_confidence_until_admin_lands -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs crates/gamelife-core/src/lib.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: settle gray-zone AI only by task id

EOF
)"
```

---

### Task 2: One task-match prompt

**Files:**
- Modify: `crates/gamelife-core/src/task_ai.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Modify: `src-tauri/src/text_ai.rs` (`build_text_ai_prompt`, `policy_names_blurb` if it becomes unused, tests from about line 194)
- Test: `crates/gamelife-core/src/task_ai.rs`

**Interfaces:**
- Consumes: `select_prompt_snapshots`, `nonempty_guides`, `truncate_guide`, `MAX_JUDGMENT_TASKS`, `CategoryGuides`, `Policy`, `TaskSnapshot`.
- Produces:

```rust
pub struct TaskMatchPrompt {
    pub text: String,
    pub shown: Vec<TaskSnapshot>,
}

pub fn build_task_match_prompt(
    snapshots: &[TaskSnapshot],
    guides: &CategoryGuides,
    policy: &Policy,
    windows: &str,
) -> TaskMatchPrompt
```

`shown` is `snapshots` when `snapshots.len() <= 20`, otherwise `select_prompt_snapshots(snapshots, &[windows])`. `text` starts with:

`Match the observed windows to at most one unfinished task. Category guides and app lists are evidence for which task fits; they are not categories to return. If none fits, task_id is null. Reply JSON {"task_id": string|null, "confidence": number}.`

Then `Tasks:` and `id=… title=… role=…` lines for `shown` only, then non-empty guide lines `mainline|side|admin|entertainment`, then non-empty list lines `主线应用` / `支线` / `杂项` / `娱乐` (at most 20 names each), then `Windows:` and `windows`. `src-tauri/src/text_ai.rs::build_text_ai_prompt` returns `build_task_match_prompt(...).text`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn prompt_includes_guides_and_skips_empty_lists() {
    let snaps = [TaskSnapshot {
        id: "t1".into(),
        title: "微信".into(),
        role: ListRole::Chore,
    }];
    let guides = CategoryGuides {
        admin: "聊天工具".into(),
        ..CategoryGuides::default()
    };
    let mut policy = default_v01();
    policy.admin_apps = vec!["微信".into()];
    policy.side_project_rules.clear();
    policy.distraction_rules.clear();
    policy.trusted_apps.clear();
    let built = build_task_match_prompt(&snaps, &guides, &policy, "app=微信 title=微信 url= document_path= idle=0");
    assert!(built.text.contains("task_id"));
    assert!(built.text.contains("id=t1 title=微信 role=chore"));
    assert!(built.text.contains("admin: 聊天工具"));
    assert!(built.text.contains("杂项: 微信"));
    assert!(!built.text.contains("支线:"));
    assert!(!built.text.contains("娱乐:"));
    assert!(!built.text.contains("主线应用:"));
    assert!(!built.text.contains("{\"category\""));
    assert_eq!(built.shown.len(), 1);
}

#[test]
fn prompt_shown_slice_is_capped_at_20() {
    let snaps: Vec<TaskSnapshot> = (0..25)
        .map(|i| TaskSnapshot {
            id: format!("t{i}"),
            title: format!("Task{i:02}"),
            role: ListRole::Mainline,
        })
        .collect();
    let built = build_task_match_prompt(&snaps, &CategoryGuides::default(), &default_v01(), "app=Cursor title=x url= document_path= idle=1");
    assert_eq!(built.shown.len(), 20);
    assert!(!built.text.contains("id=t20 "));
}
```

Update `text_ai.rs` test `prompt_omits_empty_guides_and_protected_lines_already_filtered`: an empty snapshot list must contain `task_id` and must not contain `{"category"`. Delete the assertion that the empty prompt contains `category` and not `task_id`.

- [ ] **Step 2: Run the core test and confirm it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::tests::prompt_includes_guides_and_skips_empty_lists -- --nocapture`

Expected: FAIL with `build_task_match_prompt` not found.

- [ ] **Step 3: Implement**

Add `TaskMatchPrompt` and `build_task_match_prompt` in `task_ai.rs`. Export them from `lib.rs`. Point `build_text_ai_prompt` at `.text`. Leave `policy_names_blurb` in place if other tests still call it; new prompts must not use it, because it prints empty labels.

- [ ] **Step 4: Run tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core task_ai::`

Expected: PASS.

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife text_ai::`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs crates/gamelife-core/src/lib.rs src-tauri/src/text_ai.rs
git commit -m "$(cat <<'EOF'
feat: build one task-match prompt for gray-zone AI

EOF
)"
```

---

### Task 3: Slot end uses that prompt for text and vision

**Files:**
- Modify: `crates/gamelife-core/src/vision_ctx.rs` (`build_vision_prompt` around line 102, tests around lines 319–346)
- Modify: `src-tauri/src/vision.rs` (`call_vision_endpoint` around line 352 and the `analyze_*` wrappers that call it)
- Modify: `src-tauri/src/scheduler.rs` (`finalize_slot_end_in` around lines 1531 and 1614–1668, `slot_is_final` around line 654, `finalize_ended_open_slots` around line 1410, `maybe_vision_for_gray_zone` around line 211)
- Test: `crates/gamelife-core/src/vision_ctx.rs`, `src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: `TaskMatchPrompt` from Task 2. `complete_json(endpoint, prompt, Option<&[u8]>)`. `settle_from_text_ai`. `prepare_screenshot_request`.
- Produces:

```rust
pub fn build_vision_prompt(
    ctx: &SanitizedVisionContext,
    snapshots: &[TaskSnapshot],
    guides: &CategoryGuides,
    policy: &Policy,
) -> String
```

The string is `build_task_match_prompt(snapshots, guides, policy, "").text` plus the existing activity and screenshot-context paragraphs. It does not contain `Today's main quests` or `category must be one of`.

`call_vision_endpoint` returns `Result<String, VisionCallError>` (raw model text) and takes the same `snapshots`, `guides`, and `policy` arguments. `analyze_with_chain`, `analyze_with_fallback`, `call_vision_api`, `analyze_screenshot`, `analyze_screenshot_with_chain`, and `analyze_screenshot_with_fallback` thread those arguments and return `Result<String, …>`. `maybe_vision_for_gray_zone` returns `Option<String>`. `parse_vision_json` stays for `judge_slot` unit tests. Production `finalize_slot_end_in` does not pass a `VisionResult` into `judge_slot`.

- [ ] **Step 1: Write the failing tests**

In `vision_ctx.rs`, replace `prompt_includes_main_prefix_when_provided` with a test that builds a prompt from one chore snapshot titled `微信`, an admin guide `聊天工具`, and a policy whose `admin_apps` is `["微信"]`. Assert the prompt contains `id=` and `微信`, contains `Screenshot context:`, and does not contain `Today's main quests` or `category must be one of`. Update the other `build_vision_prompt(...)` call in `history_protected_window_is_redacted_but_cursor_capture_is_ok` to pass `&[]`, `&CategoryGuides::default()`, and `&default_v01()` (or the test's existing policy).

In `scheduler.rs` tests, add:

```rust
#[test]
fn gray_vision_task_json_sets_used_vision_and_category_json_does_not() {
    let snaps = [TaskSnapshot {
        id: "chore".into(),
        title: "微信".into(),
        role: ListRole::Chore,
    }];
    let ev = empty_evidence(900);
    let chain = [compat("https://a.test/v1")];
    let missed = settle_gray_vision_task(
        &chain,
        &snaps,
        &ev,
        pending_output(900),
        |_| Ok(r#"{"category":"admin","confidence":0.95}"#.into()),
    );
    assert!(missed.is_none());
    let hit = settle_gray_vision_task(
        &chain,
        &snaps,
        &ev,
        pending_output(900),
        |_| Ok(r#"{"task_id":"chore","confidence":0.91}"#.into()),
    )
    .expect("task id settles");
    assert!(!hit.pending);
    assert_eq!(hit.dominant, Dominant::Admin);
    assert!(hit.used_vision);
    assert_eq!(hit.activity.admin, 900);
}
```

Add a DB test `pending_review_is_not_reopened_and_null_slot_is`: in-memory `migrate`, insert two slots for day `2026-09-29`. One `status='pending_review'`, `category='pending_review'`, `slot_start=0`. One `status` NULL, `slot_start=900`, `capture_status='Skipped'`. Call `finalize_ended_open_slots(&mut conn, 10_000, ScreenshotRetention::None)`. Assert the first row is still `pending_review`. Assert the second row's `status` is not NULL.

`settle_gray_vision_task` does not exist yet, so this test fails to compile.

- [ ] **Step 2: Run the vision prompt test and confirm it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core vision_ctx::tests::prompt_includes_main_prefix_when_provided -- --nocapture`

Expected: FAIL to compile, or FAIL the new assertions if you renamed in place before the implementation.

- [ ] **Step 3: Implement**

`build_vision_prompt` uses the Task 2 helper with an empty windows string, then appends the current hint-seconds and screenshot-context block. Delete the category sentence and the quest list.

`finalize_slot_end_in`:

```rust
if slot_is_final(conn, day, slot_start)? {
    return Ok(());
}
if slot_status(conn, day, slot_start)?.as_deref() == Some("pending_review") {
    return Ok(());
}
```

Do not add `pending_review` to `slot_is_final`. `review_pending_slot` must still see that status as editable.

Catch-up query in `finalize_ended_open_slots`:

```sql
SELECT day, slot_start FROM slots
 WHERE status IS NULL
   AND slot_start + 900 <= ?1
 ORDER BY day, slot_start
```

Gray zone, after the existing `judge_slot(..., vision: None)`:

```rust
let prepared = build_task_match_prompt(&tasks, &policy.category_guides, &policy, &summary);
if gray && !prepared.shown.is_empty() && !summary.is_empty() {
    output = settle_gray_text_from_bodies(output, &evidence, &prepared.shown, &chain, |ep| {
        vision::complete_json(ep, &prepared.text, None)
    });
}
if output.pending && gray && !prepared.shown.is_empty() {
    // existing screenshot match: Captured, context parses, not metadata_decidable
    // prompt = build_vision_prompt(&sanitized, &prepared.shown, &policy.category_guides, &policy)
    // but that would re-select with empty windows. Append screen context to prepared.text instead:
    //   format!("{}\n\n{}", prepared.text, screenshot_context_only(&sanitized))
    // settle_gray_vision_task sets used_vision = true on Some.
}
```

Extract the screenshot-context paragraphs into `screenshot_context_block(ctx: &SanitizedVisionContext) -> String` in `vision_ctx.rs` so the slot uses `prepared.shown` from the text summary haystack, not a second selection. `build_vision_prompt` may call that helper after its own `build_task_match_prompt`. The scheduler vision call uses `format!("{}\n\n{}", prepared.text, screenshot_context_block(&sanitized))`.

```rust
fn settle_gray_vision_task(
    chain: &[vision::VisionEndpoint],
    shown: &[TaskSnapshot],
    evidence: &SlotEvidence,
    output: JudgeOutput,
    mut body_for: impl FnMut(&vision::VisionEndpoint) -> Result<String, vision::VisionCallError>,
) -> Option<JudgeOutput> {
    vision::try_provider_chain(chain, |ep| {
        let raw = body_for(ep)?;
        let mut next = settle_from_text_ai(output.clone(), evidence, shown, &raw)
            .ok_or(vision::VisionCallError::Parse)?;
        next.used_vision = true;
        Ok(next)
    })
    .ok()
}
```

Replace the `settle_gray_vision_from_results` + `judge_slot(vision: Some(_))` block in `finalize_slot_end_in` with `settle_gray_vision_task`. JPEG bytes still come from `prepare_screenshot_request`. HTTP is `complete_json(ep, &prompt, Some(&jpeg))`. `Err(())` from a protected capture stays `None`.

Delete `gray_vision_chain_skips_low_confidence_and_still_pending`. It settles a category string without a task id. `gray_vision_task_json_sets_used_vision_and_category_json_does_not` replaces it. Do not leave a test that settles `category: admin` without a task id.

`maybe_vision_for_gray_zone` and `call_vision_endpoint`: thread `snapshots`, `guides`, and `policy` into the new prompt and return `Result<String, _>` / `Option<String>`. No production caller should parse that string as a vision category.

- [ ] **Step 4: Run tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core vision_ctx::`

Expected: PASS.

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife scheduler::tests::gray_vision_task_json_sets_used_vision_and_category_json_does_not scheduler::tests::pending_review_is_not_reopened_and_null_slot_is -- --nocapture`

Expected: PASS.

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS. This is the app-crate gate.

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/vision_ctx.rs src-tauri/src/vision.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: require a task id before vision can close a gray slot

EOF
)"
```

---

### Task 4: Settings copy and agent docs

**Files:**
- Modify: `src/pages/Settings.tsx` (category-guide caption near line 1314, 判定顺序 near lines 1943–1964)
- Modify: `CLAUDE.md` (judgment pipeline item 3, about line 81; vision fail-closed bullet, about line 115)
- Modify: `AGENTS.md` (the plan bullet that says unmatched work goes to AI, about line 46)

**Interfaces:**
- Consumes: the behavior from Tasks 1–3. No new functions.
- Produces: user-facing 判定顺序 that matches spec §6, and agent docs that no longer describe an empty-snapshot category match or a free vision category.

- [ ] **Step 1: Replace the settings strings**

Caption of 「类别说明」:

`灰字为样稿。空着保存不进提示。写了也只在灰区帮助 AI 从任务里挑选，不直接定类别。`

判定顺序 four items, in order:

1. `硬规则：锁屏或暂停 → 离开；娱乐名单 → 娱乐；闲置满 3 分钟 → 离开；GameLife → 支线；未完成任务的标题对上 → 按该任务的角色。对不上 → 待定。杂项名单和支线名单不在这一步定性。`
2. `元数据够确定就直接结算，不调 AI：匹配到的主线满 13 分钟，且支线、杂项、娱乐合计不超过 1 分钟 → 主线；这三类合计满 5 分钟且压过主线 → 归到占优的一类；离开满 10 分钟且匹配主线不足 5 分钟 → 离开。`
3. `剩下的是灰区。这一槽没有未完成任务 → 待复核，不调用 AI。有任务则文本判断必须从这些任务里点一条，置信度至少 0.7。类别说明和应用名单只帮助挑选。`
4. `文本没点中，且这一槽有可用截图：截图判断用同一份材料和同一条回复规则再点一次。点中按该任务的角色结案。没有截图、截图受保护、没点中或回复不合法 → 待复核，不猜类别。`

- [ ] **Step 2: Update the two docs**

`CLAUDE.md` item 3 becomes: with a non-empty pinned snapshot, text AI must return a `task_id` from the at most 20 tasks in the prompt (`select_prompt_snapshots`); category guides and app lists are evidence only. An empty snapshot does not call AI; the gray slot stays `pending_review`.

Item 4 becomes: vision uses that same prompt plus the screenshot and must return a `task_id`. A category JSON, a miss, a protected capture, or no screenshot leaves `pending_review`. `used_vision` is 1 only when the screenshot call settles the slot.

The vision fail-closed bullet: replace “invalid category” with “a reply that is not a confident `task_id`”.

`AGENTS.md`: after “unmatched work goes to AI”, add “AI must name an unfinished task; otherwise the slot stays pending review. Category guides do not classify a slot by themselves.”

- [ ] **Step 3: Run the frontend gate**

Run: `npx vitest run --dir src`

Expected: PASS. There is no existing assertion on these sentences; this run checks the settings edit did not break the suite.

- [ ] **Step 4: Commit**

```bash
git add src/pages/Settings.tsx CLAUDE.md AGENTS.md
git commit -m "$(cat <<'EOF'
docs: describe gray-zone AI as task match only

EOF
)"
```

The settings change is user-facing copy, so the commit also includes `src/pages/Settings.tsx`. The message stays `docs:` because the behavior landed in Tasks 1–3.

---

## Self-review

Spec §2 rows map to tasks: call timing (existing tests, Task 3 does not remove them), empty tasks (Task 3 DB/gray branch), text materials (Task 2), text reply (Task 1), vision materials and `used_vision` (Task 3), misses (Task 1 and Task 3), no screenshot / protected (existing `prepare_screenshot_request` error plus Task 3 not calling judge with a category), activity buckets (Task 1), old slots (Task 3 early return and SQL), settings copy (Task 4). Historical `research_support` labels are left on the timeline; no task deletes them.

## Execution note

Do not start Task 1 until this plan is the one the user asked to execute. Tasks are ordered: Task 3 does not compile against the prompt type until Task 2 has landed.
