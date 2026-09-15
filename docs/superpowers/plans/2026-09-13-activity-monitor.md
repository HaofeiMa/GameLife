# Activity Monitor, Analytics, and Shop Desire Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 GameLife 从自建计划本改成 AI 活动监测器：应用名单 + 一句话规则 + 只读 TickTick 快照并行判定；今日改为日报与复核；本周升格为周/月/节奏/应用统计；商店做成值得打开的货架。不在本机复刻 TickTick 日历。

**Architecture:** 判定纯函数、类别 JSON、快照证据词、报表聚合放在 `gamelife-core`。SQLite `user_version=3` 增加 `ticktick_cache`、`app_day_stats`、`host_day_stats`。槽开始钉 TickTick 快照；槽结束硬规则 → 文本 AI（有快照匹配任务 / 无快照匹配类别）→ 视觉。`resolve_slot` 同一事务写日汇总。TickTick HTTP/OAuth 与钥匙串在 `src-tauri`。前端只经 `src/lib/api.ts`。

**Tech Stack:** Tauri 2、Rust、React、TypeScript、vitest、SQLite、chrono、reqwest、keyring、sha2、base64。不引入 FullCalendar、dnd-kit、ticktick-cli。

**Spec:** 三份一起执行，不得删减条款：

- `docs/superpowers/specs/2026-09-13-activity-monitor-design.md`
- `docs/superpowers/specs/2026-09-13-analytics-design.md`
- `docs/superpowers/specs/2026-09-13-shop-desire-design.md`

未提及的观测、账本、Gold Day、永不截屏仍以 2026-09-10、Wave 1、落地判定、游戏手感为准。

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测，不是离开。`credited ≤ observed ≤ 实际槽长`。
- 一天最多 96 个 15 分钟槽。周末不采样。
- 永不截屏：不截图、不把该窗口标题/路径/URL 送进模型、该段不发币。内置项不可删。
- 账本 `UNIQUE reward_event_key`。已 `final` 的槽不改经济。报告误判只留记录。
- Gold Day（当天主线 credited ≥ 28800）之后不再产生硬币或能量。已有余额可继续兑换。
- 主线 tick：900s→1 硬币、90s→1 能量，每日上限 32/320。支线 1500s/150s，杂项 3000s/300s。不出现小数硬币。
- 商店单事务；同一 `redemption_id` 不双扣；同时最多一段未结束的能量娱乐会话。解锁仍是当天主线 credited ≥ 3600。
- 无系统通知、不申请通知权限、无音效。Habitica GPL 与 TickTick 资源不用。
- `document_path` 不得从窗口标题伪造。截图路径永不进文本 haystack。
- 视觉 fail-closed：只接受 `SanitizedVisionContext`。保护窗口禁止整段 HTTP。
- 观测优先于 TickTick 计划：娱乐规则命中就是娱乐、0 币。
- 无 TickTick 快照不再把主线 credited 打成 0；自动 Core 仍要落地证据。
- 本机无加任务、无新建列表、无自绘计划|实际双列日历、无 FullCalendar/dnd-kit。
- TickTick 只读；截图与样本标题不写回 TickTick。运行时不调用 ticktick-cli。
- API Key 与 TickTick token / Client Secret 只进钥匙串，不进 `config.json`。
- 统计一律 `SUM(activity_*)` 或样本 15s；禁止 `dominant × 15` 当周/月总量。
- 日汇总与槽决议同一事务；失败则槽不得标 final。
- 改 `src-tauri/`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife` 必须编译并跑过。
- 改 `gamelife-core`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 界面中文。能量在 UI 称「能量」，账本字段仍是 `xp_delta`。不出现 Quest。
- 每个任务提交一条英文 conventional commit，不 `--no-verify`。

---

## Spec coverage（执行前对照，禁止跳过）

| Spec 条款 | 任务 |
| --- | --- |
| F §5.1 `Hint::Admin` 与硬规则顺序 | Task 2 |
| F §5.1 快照标题证据词 | Task 3 |
| F §5.2 去掉空任务 credited=0；自动 Core 含 admin 上限 | Task 4 |
| F §5.3 双 JSON（task_id / category） | Task 5, 13 |
| F §6 Policy `admin_apps` + `category_guides` + 样稿不存盘 | Task 1, 16 |
| F §7 TickTick OAuth、映射、缓存、钉快照、>20 条 | Task 7–12 |
| F §8 导航图标、`showRailLabels`、今日日报、冻结下拉、结束今天 | Task 17–18 |
| F §8.3 设置四板块 | Task 16 |
| G §4 日汇总表 | Task 8, 14 |
| G §5 日报 | Task 18 |
| G §6.1–6.4 周/月/节奏/应用 | Task 15, 19 |
| G 点月历格切到今日且徽章仍是今天 | Task 18–19 |
| H 英雄区、大卡、进行中卡、记录、错误文案 | Task 20 |
| 不引入 FullCalendar / 本机任务 CRUD UI | Task 18 删除；本机表保留不读 |

---

## File Structure

```
crates/gamelife-core/src/policy.rs       CategoryGuides、admin_apps、截断 guides
crates/gamelife-core/src/types.rs        Hint::Admin
crates/gamelife-core/src/hint.rs         硬规则顺序、快照证据
crates/gamelife-core/src/judge.rs        analyze_slot_evidence 快照参数；自动 Core
crates/gamelife-core/src/task.rs         tokenize、tt- id、hashtag 角色
crates/gamelife-core/src/task_ai.rs      parse_category_match_json、apply_category_match
crates/gamelife-core/src/reports.rs      达标率、娱乐连段、环比、热力格子
crates/gamelife-core/src/app_stats.rs    样本 → App/host 秒（纯函数）
crates/gamelife-core/src/vision_ctx.rs   HintSeconds.admin
crates/gamelife-core/src/lib.rs          导出
src-tauri/src/db.rs                     user_version=3 三张表
src-tauri/src/keychain.rs               TickTick token / client secret
src-tauri/src/config.rs                 showRailLabels、adminApps、guides、projectRoles、clientId
src-tauri/src/ticktick.rs               OAuth PKCE、拉取、缓存、映射
src-tauri/src/text_ai.rs                prompt 含 guides；空快照走类别
src-tauri/src/scheduler.rs              PolicyJson 新字段；钉 TickTick 快照；灰区双路径；metadata_decidable
src-tauri/src/resolve.rs                事务内写 app_day_stats / host_day_stats
src-tauri/src/commands.rs               设置保存、TickTick 命令、报表命令
src-tauri/src/lib.rs                    注册命令
src-tauri/Cargo.toml                    sha2
src/lib/api.ts                          类型与 invoke
src/lib/guides.ts                       四句灰字样稿（不提交到 Policy 的占位）
src/lib/shopUnlock.ts                   距解锁分钟
src/App.tsx                             SVG 图标、showRailLabels
src/pages/Settings.tsx                  名单改名、guides、TickTick 板块
src/pages/Today.tsx                     去掉计划本；日报 + 单列时间轴 + 冻结下拉
src/pages/Week.tsx                      统计四段（文件可仍叫 Week.tsx）
src/pages/Shop.tsx                      英雄区与大卡
src/styles.css                          日报、统计、商店、rail
```

本机 `tasks` / `task_lists` 表不删。`upsert_task` / `create_list` / `parse_task_line` 命令保留但不从 UI 调用。

---

### Task 1: Policy 新字段与旧 JSON 兼容

**Files:**
- Modify: `crates/gamelife-core/src/policy.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Modify: 所有 `Policy {` 字面量：`crates/gamelife-core/src/hint.rs`、`judge.rs`、`vision_ctx.rs`、`src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: 现有 `Policy`
- Produces:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryGuides {
    #[serde(default)]
    pub mainline: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub admin: String,
    #[serde(default)]
    pub entertainment: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub trusted_apps: Vec<String>,
    pub distraction_rules: Vec<String>,
    pub side_project_rules: Vec<String>,
    pub reading_apps: Vec<String>,
    pub never_capture_apps: Vec<String>,
    #[serde(default)]
    pub admin_apps: Vec<String>,
    #[serde(default)]
    pub category_guides: CategoryGuides,
}

pub fn truncate_guide(s: &str) -> String {
    s.chars().take(500).collect()
}

pub fn nonempty_guides(g: &CategoryGuides) -> Vec<(&'static str, String)>
```

`nonempty_guides` 只返回 trim 后非空的句，键为 `mainline`/`side`/`admin`/`entertainment`。`default_v01()` 的新字段为空。

每个已有 `Policy { ... }` 补：

```rust
admin_apps: vec![],
category_guides: CategoryGuides::default(),
```

- [ ] **Step 1: Write the failing test**

在 `policy.rs` `tests` 追加：

```rust
#[test]
fn old_policy_json_without_new_fields_deserializes() {
    let json = r#"{"trusted_apps":["Cursor"],"distraction_rules":[],"side_project_rules":[],"reading_apps":[],"never_capture_apps":[]}"#;
    let p: Policy = serde_json::from_str(json).unwrap();
    assert_eq!(p.trusted_apps, vec!["Cursor"]);
    assert!(p.admin_apps.is_empty());
    assert!(p.category_guides.mainline.is_empty());
}

#[test]
fn truncate_guide_caps_at_500_chars() {
    let s: String = std::iter::repeat('研').take(501).collect();
    assert_eq!(truncate_guide(&s).chars().count(), 500);
}

#[test]
fn nonempty_guides_skips_blank() {
    let g = CategoryGuides {
        mainline: "  主线说明  ".into(),
        side: " \n".into(),
        admin: String::new(),
        entertainment: "娱乐说明".into(),
    };
    let v = nonempty_guides(&g);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0], ("mainline", "主线说明".into()));
    assert_eq!(v[1], ("entertainment", "娱乐说明".into()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- policy::tests::old_policy_json_without_new_fields_deserializes`

Expected: FAIL（字段不存在或反序列化失败）

- [ ] **Step 3: Write minimal implementation**

按 Interfaces 改 `Policy` / `default_v01`，实现 `truncate_guide`、`nonempty_guides`（trim）。`lib.rs` 再导出 `CategoryGuides, truncate_guide, nonempty_guides`。补全所有 `Policy {` 字面量。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- policy`

Expected: PASS。再跑 `cargo test --offline -p gamelife-core` 确认字面量已齐。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/policy.rs crates/gamelife-core/src/lib.rs crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs crates/gamelife-core/src/vision_ctx.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: extend policy with admin apps and category guides

Keep old policy JSON loadable so slot snapshots opened
before this version still deserialize.
EOF
)"
```

---

### Task 2: `Hint::Admin` 与硬规则顺序

**Files:**
- Modify: `crates/gamelife-core/src/types.rs`
- Modify: `crates/gamelife-core/src/hint.rs`
- Modify: `crates/gamelife-core/src/judge.rs`（`accumulate_hint_activity`）
- Modify: `crates/gamelife-core/src/vision_ctx.rs`（`HintSeconds.admin` 与 match）

**Interfaces:**
- Consumes: `Policy.admin_apps`、`matches_app_identity`
- Produces: `Hint::Admin`；`hint_sample` 顺序：Away → Distraction → Admin → Side → 阅读 → 主线候选 → Unsure

`accumulate_hint_activity`：`Hint::Admin => activity.admin += secs`。

`HintSeconds` 增加 `pub admin: i64`（Default 为 0）。`activity_summary_for_vision` 的 match 增加 `Hint::Admin => hint_seconds.admin += secs`。

- [ ] **Step 1: Write the failing test**

在 `hint.rs` `tests`：

```rust
#[test]
fn admin_app_beats_trusted_but_loses_to_distraction() {
    let p = Policy {
        trusted_apps: vec!["Mail".into()],
        distraction_rules: vec!["youtube.com".into()],
        side_project_rules: vec![],
        reading_apps: vec![],
        never_capture_apps: vec![],
        admin_apps: vec!["Mail".into()],
        category_guides: CategoryGuides::default(),
    };
    assert_eq!(
        hint_sample(&sample("Mail", "Inbox", 5), &p, &[], None),
        Hint::Admin
    );
    let mut yt = sample("Google Chrome", "YouTube", 5);
    yt.url = Some("https://www.youtube.com/watch?v=1".into());
    assert_eq!(hint_sample(&yt, &p, &[], None), Hint::Distraction);
}

#[test]
fn gamelife_remains_side_even_if_listed_as_admin() {
    let p = Policy {
        trusted_apps: vec![],
        distraction_rules: vec![],
        side_project_rules: builtin_side_project_rules(),
        reading_apps: vec![],
        never_capture_apps: vec![],
        admin_apps: vec!["GameLife".into()],
        category_guides: CategoryGuides::default(),
    };
    assert_eq!(
        hint_sample(&sample("GameLife", "Today", 5), &p, &[], None),
        Hint::Admin
    );
}
```

第二个测试按 spec 顺序：Admin 在 Side 之前，故名单里的 GameLife 若同时在 `admin_apps` 会变成 Admin。Spec 写的是内置 GameLife 走 side 规则。**实现必须让内置支线（`all_side_project_rules`）在 Admin 之前还是之后？** Spec F §5.1 明确：2 娱乐 → 3 Admin → 4 Side。因此 GameLife 只有在不进 `admin_apps` 时才是 Side。第二个测试应改为：

```rust
#[test]
fn builtin_gamelife_is_side_when_not_in_admin_list() {
    let p = Policy {
        trusted_apps: vec![],
        distraction_rules: vec![],
        side_project_rules: vec![],
        reading_apps: vec![],
        never_capture_apps: vec![],
        admin_apps: vec![],
        category_guides: CategoryGuides::default(),
    };
    assert_eq!(
        hint_sample(&sample("GameLife", "Today", 5), &p, &[], None),
        Hint::Side
    );
}
```

保留现有 `gamelife_is_side_and_not_removable_from_side_rules`，补上新字段。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- hint::tests::admin_app_beats_trusted_but_loses_to_distraction`

Expected: FAIL（无 `Hint::Admin` 或不匹配）

- [ ] **Step 3: Write minimal implementation**

`types.rs` 在 `Side` 后插入 `Admin`。`hint_sample` 在 distraction 之后、side 之前：

```rust
if matches_app_identity(&sample.app, sample.bundle_id.as_deref(), &policy.admin_apps) {
    return Hint::Admin;
}
```

`judge.rs` `accumulate_hint_activity` 与 `vision_ctx.rs` match 补 `Admin`（非穷尽 match 会编译失败，必须改完）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- hint vision_ctx judge::tests`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/types.rs crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs crates/gamelife-core/src/vision_ctx.rs
git commit -m "$(cat <<'EOF'
feat: classify admin apps before side hints

Mail-like tools should count as chores, but entertainment
hosts still win so a YouTube tab cannot become admin.
EOF
)"
```

---

### Task 3: 主线快照标题作为 Core 证据词

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/hint.rs`
- Modify: `crates/gamelife-core/src/judge.rs`（`analyze_slot_evidence` 与所有调用）
- Modify: `src-tauri/src/scheduler.rs`（`compute_slot_activity`、finalize 里的 `analyze_slot_evidence`）

**Interfaces:**
- Consumes: `TaskSnapshot`
- Produces:

```rust
pub fn tokenize_title(title: &str) -> Vec<String>
pub fn snapshot_evidence_quests(snapshots: &[TaskSnapshot]) -> Vec<Quest>
```

`tokenize_title`：按 Unicode 空白与 `,，。；;|/\\` 切开，去掉 `#` 标签片段，保留长度 ≥ 2 的 token。

`snapshot_evidence_quests`：只把 `role == Mainline` 的快照变成 `Quest { text: title, evidence: tokenize_title(title), hero: false }`。

```rust
pub fn analyze_slot_evidence(
    samples: &[Sample],
    policy: &Policy,
    quests: &[Quest],
    snapshots: &[TaskSnapshot],
    slot_start: i64,
    slot_end: i64,
) -> SlotEvidence
```

内部：`let mut q = quests.to_vec(); q.extend(snapshot_evidence_quests(snapshots));` 再交给现有 `hint_sample` / `is_grounded_core_sample`。所有调用点补 `snapshots` 参数；没有快照时传 `&[]`。`hint_sample` 签名不变。

- [ ] **Step 1: Write the failing test**

在 `task.rs` tests：

```rust
#[test]
fn tokenize_title_drops_hash_and_short_tokens() {
    let t = tokenize_title("RAIDS+ 讨论 #杂项");
    assert!(t.iter().any(|x| x == "RAIDS+"));
    assert!(t.iter().any(|x| x == "讨论"));
    assert!(!t.iter().any(|x| x.contains('#')));
}
```

在 `hint.rs` 或 `judge.rs` tests：

```rust
#[test]
fn mainline_snapshot_title_grounds_path() {
    let p = pol(); // 已含 Cursor 主线应用
    let snaps = [TaskSnapshot {
        id: "tt-1".into(),
        title: "HDP train".into(),
        role: ListRole::Mainline,
    }];
    let mut s = sample("Cursor", "train.py", 5);
    s.document_path = Some("/proj/HDP/train.py".into());
    let ev = analyze_slot_evidence(
        &[s],
        &p,
        &[],
        &snaps,
        0,
        15,
    );
    assert!(ev.grounded_strong_core_seconds > 0);
}
```

把 `sample`/`pol` 按该测试模块现有 helper 对齐；若在 `task.rs` 测 tokenize 即可，grounded 测试放 `judge.rs`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- task::tests::tokenize_title_drops_hash_and_short_tokens`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 tokenize / snapshot_evidence_quests；改 `analyze_slot_evidence` 签名并修所有调用（`judge.rs` 内 `judge_slot` 传 `input.tasks`；scheduler 两处）。导出新函数。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core`

Expected: PASS

再：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- scheduler::`

Expected: PASS（签名已改）

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs crates/gamelife-core/src/lib.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: treat mainline snapshot titles as core evidence

Timed TickTick (or legacy) mainline titles can ground a
path or URL the same way Quest keywords used to.
EOF
)"
```

---

### Task 4: 自动 Core 不再要求非空任务；杂项计入干扰上限

**Files:**
- Modify: `crates/gamelife-core/src/judge.rs`

**Interfaces:**
- Consumes: `JudgeInput.tasks` 仍用于证据（Task 3），不再用于「空则 credited=0」
- Produces: `judge_slot` 自动 Core 条件：

```
grounded_strong_core >= 780
&& activity.side + activity.admin + activity.distraction <= 60
```

删除 `if tasks_empty { credited = 0; ... }`。删除自动 Core 分支上的 `!tasks_empty &&`。

- [ ] **Step 1: Write the failing test**

在 `judge.rs` tests 追加（沿用 `grid` / `pol`）：

```rust
#[test]
fn empty_snapshots_still_auto_core_when_grounded() {
    let samples = grid("Cursor", "paper", 0, 60, 15, 2);
    let out = judge_slot(JudgeInput {
        slot_start: 0,
        slot_end: 900,
        samples: &samples,
        quests: &[],
        tasks: &[],
        policy: &pol(),
        capture: CaptureStatus::Skipped,
        vision: None,
        manual_core: None,
    });
    assert!(!out.pending);
    assert_eq!(out.dominant, Dominant::CoreResearch);
    assert!(out.credited_core_seconds >= 780);
}

#[test]
fn admin_minutes_block_auto_core() {
    let mut samples = grid("Cursor", "paper", 0, 52, 15, 2);
    for i in 0..8 {
        samples.push(Sample {
            ts: 780 + i * 15,
            app: "Mail".into(),
            window_title: "Inbox".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle_seconds: 2,
            screen_locked: false,
            paused: false,
            secure_input: false,
        });
    }
    let mut policy = pol();
    policy.admin_apps = vec!["Mail".into()];
    let out = judge_slot(JudgeInput {
        slot_start: 0,
        slot_end: 900,
        samples: &samples,
        quests: &[],
        tasks: &[],
        policy: &policy,
        capture: CaptureStatus::Skipped,
        vision: None,
        manual_core: None,
    });
    assert_ne!(out.dominant, Dominant::CoreResearch);
}
```

若现网 `grid` 已带 `/paper/main.tex` 路径，空快照应能 grounded（H2 或路径）。`pol()` 的 Cursor 在 trusted_apps。标题 `paper` 无 quest 时，路径仍要能 `is_grounded_core_sample`——当前实现是路径含 quest evidence 或 H2 URL。`/paper/main.tex` 在无 quest 时 **不能** grounded。

因此空快照自动 Core 测试必须给 H2 URL 或把路径做成含 token 的快照。按 spec：无快照时靠 H2 或非空工作路径。检查 `is_grounded_core_sample`：无 quest 时只有 `host_is_research`。测试应：

```rust
let mut samples = grid("Cursor", "Overleaf", 0, 60, 15, 2);
for s in &mut samples {
    s.document_path = None;
    s.url = Some("https://www.overleaf.com/project/abc".into());
}
```

`grid` 里默认 path 要清掉，避免误以为 path 证据。Chrome/Safari 才是浏览器；Cursor + overleaf URL 仍算 haystack。`host_is_research` 看 URL。Trusted Cursor + overleaf URL → grounded。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- judge::tests::empty_snapshots_still_auto_core_when_grounded`

Expected: FAIL（仍因 `tasks_empty` 把 credited 置 0）

- [ ] **Step 3: Write minimal implementation**

按上面删门、改干扰上限。注释里「tasks empty → 0」一并改掉。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- judge::`

Expected: PASS。若旧测试依赖「空任务 credited=0」，改为断言「无落地则非自动 Core」，不要恢复空任务门。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/judge.rs
git commit -m "$(cat <<'EOF'
feat: allow auto-core without a task snapshot

App lists and grounded hosts can pay mainline when TickTick
is empty; admin time still blocks the automatic path.
EOF
)"
```

---

### Task 5: 空快照类别 JSON 与 `apply_category_match`

**Files:**
- Modify: `crates/gamelife-core/src/task_ai.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: `JudgeOutput`、`SlotEvidence`、`TASK_MATCH_MIN`（0.7）
- Produces:

```rust
pub struct CategoryMatch {
    pub dominant: Dominant,
    pub confidence: f64,
}

pub enum CategoryMatchError {
    InvalidJson,
    BadConfidence,
    BadCategory,
}

pub fn parse_category_match_json(json: &str) -> Result<Option<CategoryMatch>, CategoryMatchError>

pub fn apply_category_match(
    output: JudgeOutput,
    ev: &SlotEvidence,
    m: &CategoryMatch,
) -> JudgeOutput
```

`parse_category_match_json`：

- `confidence` 同 `parse_task_match_json` 规则
- `category: null` → `Ok(None)`
- 字符串映射：`core_research`→`CoreResearch`，`research_support`→`ResearchSupport`，`side_project`→`SideProject`，`admin`→`Admin`，`distraction`→`Distraction`；其余 `BadCategory`

`apply_category_match`：

- `confidence < 0.7` → 原样返回
- `CoreResearch` 且 `ev.strong_core_seconds == 0` → `pending = true`，`credited_* = 0`，`dominant = PendingReview`
- `CoreResearch` 且 `strong_core > 0` → 与 `apply_task_match` 主线相同（`payout_base_seconds`）
- `ResearchSupport` → `credited_* = 0`，`activity.support = activity.support.max(payout_base_seconds(ev))`，`pending = false`
- `SideProject` → 同 `apply_task_match` 的 Side
- `Admin` → 同 Chore
- `Distraction` → `credited_* = 0`，`dominant = Distraction`，`pending = false`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn category_null_is_none() {
    assert_eq!(parse_category_match_json(r#"{"category":null,"confidence":0.9}"#), Ok(None));
}

#[test]
fn core_without_strong_core_is_pending() {
    let ev = SlotEvidence {
        hints: vec![],
        spans: vec![],
        activity: ActivitySeconds::default(),
        strong_core_seconds: 0,
        grounded_strong_core_seconds: 0,
        reading_bridge_seconds: 0,
        observed_seconds: 900,
    };
    let out = apply_category_match(
        JudgeOutput {
            dominant: Dominant::Unknown,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: 900,
            used_vision: false,
            pending: true,
        },
        &ev,
        &CategoryMatch { dominant: Dominant::CoreResearch, confidence: 0.9 },
    );
    assert!(out.pending);
    assert_eq!(out.credited_core_seconds, 0);
}

#[test]
fn support_does_not_pay() {
    let mut ev = /* observed 900, away 0 distraction 0 */;
    ev.observed_seconds = 900;
    ev.activity.away = 0;
    ev.activity.distraction = 0;
    let out = apply_category_match(empty_output(900), &ev, &CategoryMatch {
        dominant: Dominant::ResearchSupport,
        confidence: 0.9,
    });
    assert_eq!(out.credited_core_seconds, 0);
    assert_eq!(out.dominant, Dominant::ResearchSupport);
    assert!(!out.pending);
}
```

`empty_output` 在测试模块里写一个小函数，不要用伪代码注释；`SlotEvidence` 必须填满所有字段。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- task_ai::tests::category_null_is_none`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

按 Interfaces 实现并 `pub use`。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- task_ai`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: parse category JSON when the slot has no tasks

Guides-only AI must not auto-pay core without strong
core evidence; support stays unpaid.
EOF
)"
```

---

### Task 6: `metadata_decidable` 与自动 Core 对齐

**Files:**
- Modify: `src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: 与 `judge_slot` 相同的秒数门槛常量（780 / 60）
- Produces: `metadata_decidable` 自动 Core 臂：

```
grounded_strong_core >= 780
&& activity.side + activity.admin + activity.distraction <= 60
```

删除 `!quests_empty &&`。保留参数 `quests_empty` 会变死参数——**删掉该参数**，更新所有调用与测试（`metadata_decidable_strong_core_path` 等）。

- [ ] **Step 1: Write the failing test**

改 `scheduler.rs` 现有 `metadata_decidable_strong_core_path`：在 `quests_empty: true` 且 grounded 780、side+admin+distraction≤60 时必须 `true`（现在若依赖 `!quests_empty` 会是 false）。把测试改成不再传 `quests_empty`。

另加：

```rust
#[test]
fn metadata_decidable_admin_blocks_auto_core_skip_vision() {
    let mut activity = ActivitySeconds::default();
    activity.admin = 90;
    assert!(!metadata_decidable(&activity, 800, 800, 0, 900));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- scheduler::tests::metadata_decidable_admin_blocks_auto_core_skip_vision`

Expected: FAIL（参数未改或 admin 未计入）

- [ ] **Step 3: Write minimal implementation**

改函数签名与调用点（finalize 里 `metadata_decidable(...)` 去掉 `!quest_list_has_evidence(&quests)`）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- scheduler::tests::metadata_decidable`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
fix: skip vision on grounded auto-core even without quests

Metadata decidable must match judge_slot so title-only
gray still gets a screenshot when auto-core does not fire.
EOF
)"
```

---

### Task 7: TickTick 标题角色、id 前缀、判定集合纯函数

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: `ListRole`、`MAX_JUDGMENT_TASKS`、`align_range`
- Produces:

```rust
pub fn ticktick_snapshot_id(raw_id: &str) -> String  // "tt-{raw}"，已有 tt- 前缀不重复加

pub fn role_from_hashtag(title: &str, fallback: ListRole) -> ListRole
// #主线/#主线任务 → Mainline；#支线/#支线任务 → Side；#长期/#长期计划 → Longterm；#杂项 → Chore；无法解析则 fallback

pub struct TimedTask {
    pub id: String,
    pub title: String,
    pub role: ListRole,
    pub start: i64,
    pub end: i64,
    pub done: bool,
}

pub fn ticktick_judgment_set(tasks: &[TimedTask], day_start: i64, day_end: i64) -> Vec<&TimedTask>
```

`ticktick_judgment_set`：`!done && start < day_end && end > day_start`，长期无时段的不会出现在 `TimedTask`（无 start/end 的根本不构造）。超过 20 条时 **返回空 Vec**（调用方钉 `[]` 并走空快照分支），不 Err。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn ticktick_id_is_prefixed_once() {
    assert_eq!(ticktick_snapshot_id("abc"), "tt-abc");
    assert_eq!(ticktick_snapshot_id("tt-abc"), "tt-abc");
}

#[test]
fn hashtag_overrides_project_role() {
    assert_eq!(role_from_hashtag("讨论 #杂项", ListRole::Mainline), ListRole::Chore);
    assert_eq!(role_from_hashtag("讨论", ListRole::Side), ListRole::Side);
}

#[test]
fn more_than_twenty_timed_tasks_yields_empty_set() {
    let tasks: Vec<TimedTask> = (0..21)
        .map(|i| TimedTask {
            id: format!("{i}"),
            title: format!("t{i}"),
            role: ListRole::Mainline,
            start: 1000,
            end: 1900,
            done: false,
        })
        .collect();
    assert!(ticktick_judgment_set(&tasks, 0, 86400).is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- task::tests::more_than_twenty_timed_tasks_yields_empty_set`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现三个函数并导出。`role_from_hashtag` 复用 `task_parse.rs` 的列表别名逻辑（可抽 `match_role_alias(tag: &str) -> Option<ListRole>` 到 `task.rs`，`task_parse` 改为调用它，避免两套中文别名）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- task task_parse`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/task_parse.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: map TickTick titles into a capped judgment set

Over twenty timed tasks fail open to app-list judgment
instead of blocking sampling.
EOF
)"
```

---

### Task 8: SQLite user_version 3 与三张新表

**Files:**
- Modify: `src-tauri/src/db.rs`

**Interfaces:**
- Consumes: 现有 `migrate`
- Produces: `TARGET_USER_VERSION = 3`。`version < 3` 时 `CREATE TABLE IF NOT EXISTS`：

```sql
CREATE TABLE IF NOT EXISTS ticktick_cache (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  title TEXT NOT NULL,
  role TEXT NOT NULL,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_day_stats (
  day TEXT NOT NULL,
  app TEXT NOT NULL,
  bundle_id TEXT NOT NULL DEFAULT '',
  samples INTEGER NOT NULL,
  idle_seconds INTEGER NOT NULL,
  core INTEGER NOT NULL,
  support INTEGER NOT NULL,
  admin INTEGER NOT NULL,
  side INTEGER NOT NULL,
  distraction INTEGER NOT NULL,
  away INTEGER NOT NULL,
  unobserved INTEGER NOT NULL,
  protected INTEGER NOT NULL,
  PRIMARY KEY (day, app, bundle_id)
);

CREATE TABLE IF NOT EXISTS host_day_stats (
  day TEXT NOT NULL,
  host TEXT NOT NULL,
  samples INTEGER NOT NULL,
  core INTEGER NOT NULL,
  support INTEGER NOT NULL,
  admin INTEGER NOT NULL,
  side INTEGER NOT NULL,
  distraction INTEGER NOT NULL,
  PRIMARY KEY (day, host)
);
```

新库 `SCHEMA` 常量也要包含这三张表，避免 `user_version` 已是 3 的全新库漏表。迁移在 `if version < TARGET` 之后追加 `if version < 3` 建表再 `pragma user_version=3`。注意现网用 `if version < 2` 升到 2；改成：`<2` 做 Wave1 列并设到 2，然后 `<3` 建三表并设到 3。或一次性：若 `<3` 则确保 Wave1 列 + 三表，最后写 3。

- [ ] **Step 1: Write the failing test**

在 `db.rs` 或 `commands.rs` tests 旁的 db tests 模块（若无则在 `db.rs` 加 `#[cfg(test)]`）：

```rust
#[test]
fn migrate_creates_monitor_tables_and_version_3() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 3);
    conn.execute("INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at) VALUES ('tt-1','p','t','mainline',1,2,3)", []).unwrap();
    conn.execute("INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected) VALUES ('2026-09-13','Cursor','',1,0,15,0,0,0,0,0,0,0)", []).unwrap();
    conn.execute("INSERT INTO host_day_stats (day, host, samples, core, support, admin, side, distraction) VALUES ('2026-09-13','arxiv.org',1,15,0,0,0,0)", []).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- db::tests::migrate_creates_monitor_tables_and_version_3`

Expected: FAIL（无模块则先放 `commands::tests` 并 `use crate::db::migrate`）

- [ ] **Step 3: Write minimal implementation**

改 `SCHEMA` 与 `migrate`。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- migrate_creates_monitor_tables_and_version_3`

Expected: PASS。全量 `cargo test --offline -p gamelife` 也要绿（旧 migrate 测试若断言 version==2 改为 3）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "$(cat <<'EOF'
feat: add TickTick cache and daily app rollup tables

Version 3 keeps sample retention at seven days while
monthly app reports still have per-day hint seconds.
EOF
)"
```

---

### Task 9: 钥匙串与 `AppSettings` 外壳字段

**Files:**
- Modify: `src-tauri/src/keychain.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src/lib/api.ts`（`AppSettings` 类型先对齐，UI 下一任务再用）

**Interfaces:**
- Consumes: 现有 keyring 封装
- Produces:

```rust
pub const KEYCHAIN_SERVICE_TICKTICK_TOKEN: &str = "ma.haofei.gamelife.ticktick";
pub const KEYCHAIN_SERVICE_TICKTICK_SECRET: &str = "ma.haofei.gamelife.ticktick-secret";
pub const KEYCHAIN_ACCOUNT_ACCESS: &str = "access";
pub const KEYCHAIN_ACCOUNT_REFRESH: &str = "refresh";

pub fn get_ticktick_access_token() -> Result<String, String>
pub fn set_ticktick_access_token(v: &str) -> Result<(), String>
pub fn get_ticktick_refresh_token() -> Result<String, String>
pub fn set_ticktick_refresh_token(v: &str) -> Result<(), String>
pub fn get_ticktick_client_secret() -> Result<String, String>
pub fn set_ticktick_client_secret(v: &str) -> Result<(), String>
pub fn clear_ticktick_tokens() -> Result<(), String>
```

`AppSettings` 增加（serde camelCase + default）：

```rust
#[serde(default = "default_true")]
pub show_rail_labels: bool,
#[serde(default)]
pub admin_apps: Vec<String>,
#[serde(default)]
pub category_guides: CategoryGuides, // 或内嵌四字段；必须与 Policy 同结构以便 save_settings 写入政策
#[serde(default)]
pub ticktick_client_id: String,
#[serde(default)]
pub ticktick_project_roles: std::collections::BTreeMap<String, String>, // projectId → mainline|side|longterm|chore|ignore
```

`default_settings()`：`show_rail_labels: true`，其余空。`CategoryGuides` 已在 core，config 可 `use gamelife_core::CategoryGuides`。

`src/lib/api.ts`：

```ts
export interface CategoryGuides {
  mainline: string;
  side: string;
  admin: string;
  entertainment: string;
}
export interface AppSettings {
  // 现有字段保留
  showRailLabels: boolean;
  adminApps: string[];
  categoryGuides: CategoryGuides;
  ticktickClientId: string;
  ticktickProjectRoles: Record<string, string>;
}
```

- [ ] **Step 1: Write the failing test**

`keychain.rs`：

```rust
#[test]
fn ticktick_services_are_distinct() {
    assert_ne!(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_SERVICE);
    assert_ne!(KEYCHAIN_SERVICE_TICKTICK_SECRET, KEYCHAIN_SERVICE_TICKTICK_TOKEN);
}
```

`config.rs` tests：

```rust
#[test]
fn old_config_json_defaults_rail_labels_true() {
    let parsed: AppSettings = serde_json::from_str(
        r#"{"screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,"trustedApps":[],"distractionRules":[],"sideProjectRules":[],"readingApps":[],"neverCaptureApps":[]}"#,
    )
    .unwrap();
    assert!(parsed.show_rail_labels);
    assert!(parsed.admin_apps.is_empty());
    assert!(parsed.ticktick_client_id.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- config::tests::old_config_json_defaults_rail_labels_true`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

加字段与钥匙串函数。`default_settings` 补全。`api.ts` 同步类型（可先不改 Settings UI）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- config:: keychain::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/keychain.rs src-tauri/src/config.rs src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: store TickTick secrets in keychain and extend settings

Rail label visibility and project-role maps belong in
config.json; tokens never do.
EOF
)"
```

---

### Task 10: TickTick Open API JSON → `TimedTask`（无网络）

**Files:**
- Create: `src-tauri/src/ticktick.rs`
- Modify: `src-tauri/src/lib.rs`（`pub mod ticktick`）

**Interfaces:**
- Consumes: Task 7 函数、`align_range`
- Produces:

```rust
#[derive(Deserialize)]
pub struct OpenTask {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub status: Option<i64>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub time_zone: Option<String>,
    pub is_all_day: Option<bool>,
}

pub fn parse_open_project_data_tasks(json: &str) -> Result<Vec<OpenTask>, String>
pub fn open_task_to_timed(
    task: &OpenTask,
    fallback_role: ListRole,
    default_tz: &chrono::FixedOffset,
) -> Option<TimedTask>
```

`parse_open_project_data_tasks`：对象可含 `tasks` 数组；也可直接是数组。

`open_task_to_timed`：`status==2` 或 `is_all_day==true` 或缺 start/due → `None`。解析 `"yyyy-MM-dd'T'HH:mm:ssZ"`（TickTick 文档格式，含 `+0000`）。`role_from_hashtag(&title, fallback)`。id 经 `ticktick_snapshot_id`。时间 `align_range`。

- [ ] **Step 1: Write the failing test**

在 `ticktick.rs` `#[cfg(test)]`：

```rust
#[test]
fn skips_all_day_and_completed() {
    let json = r#"{"tasks":[
      {"id":"1","title":"A","status":0,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000","timeZone":"UTC"},
      {"id":"2","title":"B","status":2,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"},
      {"id":"3","title":"C","status":0,"isAllDay":true,"startDate":"2026-09-13T00:00:00+0000","dueDate":"2026-09-13T00:00:00+0000"}
    ]}"#;
    let tasks = parse_open_project_data_tasks(json).unwrap();
    let tz = chrono::FixedOffset::east_opt(0).unwrap();
    let timed: Vec<_> = tasks.iter().filter_map(|t| open_task_to_timed(t, ListRole::Mainline, &tz)).collect();
    assert_eq!(timed.len(), 1);
    assert_eq!(timed[0].id, "tt-1");
    assert!(timed[0].end > timed[0].start);
}

#[test]
fn hashtag_chore_from_title() {
    let t = OpenTask {
        id: "9".into(),
        project_id: None,
        title: "报销 #杂项".into(),
        status: Some(0),
        start_date: Some("2026-09-13T01:00:00+0000".into()),
        due_date: Some("2026-09-13T02:00:00+0000".into()),
        time_zone: Some("UTC".into()),
        is_all_day: Some(false),
    };
    let tz = chrono::FixedOffset::east_opt(0).unwrap();
    let got = open_task_to_timed(&t, ListRole::Mainline, &tz).unwrap();
    assert_eq!(got.role, ListRole::Chore);
}
```

OpenTask 字段 serde rename：`#[serde(rename_all = "camelCase")]` 以匹配 `startDate`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- ticktick::tests::skips_all_day_and_completed`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现解析。日期用 `chrono::DateTime::parse_from_str(..., "%Y-%m-%dT%H:%M:%S%z")`，`+0000` 无冒号时用 `%z`。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- ticktick::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ticktick.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: parse TickTick Open API tasks into timed snapshots

All-day and completed items stay out of the judgment set.
EOF
)"
```

---

### Task 11: 缓存 upsert、OAuth PKCE、同步命令

**Files:**
- Modify: `src-tauri/src/ticktick.rs`
- Modify: `src-tauri/Cargo.toml`（`sha2 = "0.10"`）
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`（注册命令）
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: Task 8 表、Task 9 钥匙串、Task 10 解析、`AppSettings.ticktick_project_roles`
- Produces:

```rust
pub fn pkce_verifier() -> String
pub fn pkce_challenge(verifier: &str) -> String
pub fn parse_token_response(json: &str) -> Result<(String, Option<String>), String>

pub fn replace_ticktick_cache(conn: &Connection, rows: &[TimedTask], fetched_at: i64) -> Result<(), DbOpError>
pub fn load_ticktick_cache(conn: &Connection) -> Result<Vec<TimedTask>, DbOpError>
pub fn cache_is_fresh(fetched_at_max: Option<i64>, now: i64) -> bool // now - max <= 300

pub fn project_role(map: &BTreeMap<String, String>, project_id: &str) -> Option<ListRole>
// ignore 或缺失 → None（该清单不进缓存）
```

Tauri 命令（rename camelCase）：

- `ticktick_status() -> { connected: bool, lastSync: number | null }`
- `ticktick_set_client_secret(secret: String)`
- `ticktick_begin_oauth() -> { authorizeUrl: String }`（把 verifier 放进程内 `Mutex<Option<String>>` 静态，仅用于紧随其后的 callback）
- `ticktick_finish_oauth(callbackUrl: String)`
- `ticktick_disconnect()`
- `ticktick_sync()` → 拉清单、按角色过滤、写缓存
- `ticktick_list_projects() -> Vec<{ id, name, role }>`

HTTP 抽：

```rust
pub trait TickTickHttp {
    fn get_json(&self, url: &str, bearer: Option<&str>) -> Result<String, String>;
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, String>;
}
```

生产用 reqwest；单测用假客户端。`ticktick_sync_with(http, conn, settings, now, access)` 供测试注入。生产 GET 超时 **2 秒**（spec F §7.4）；HTTP 429 时该次 sync 失败并保留旧缓存，下次间隔至少 60 秒（可用 `app_meta` 键 `ticktick_backoff_until`）。禁止忙循环重试打爆 API。

OAuth：`https://ticktick.com/oauth/authorize?client_id=...&redirect_uri=http://127.0.0.1:{port}/callback&response_type=code&scope=tasks:read&code_challenge=...&code_challenge_method=S256`。本机 `TcpListener` 接一发 GET。token `https://ticktick.com/oauth/token`。只调用 GET project / project data，禁止 POST task。

`begin_oauth` 可在测试中只测 `pkce_challenge` 与 `parse_token_response`，不必真开端口。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn pkce_challenge_is_s256_unpadded() {
    let chal = pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert_eq!(chal, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
}

#[test]
fn cache_freshness_five_minutes() {
    assert!(cache_is_fresh(Some(1000), 1299));
    assert!(!cache_is_fresh(Some(1000), 1301));
    assert!(!cache_is_fresh(None, 10));
}

#[test]
fn ignore_role_excludes_project() {
    let mut map = BTreeMap::new();
    map.insert("p1".into(), "ignore".into());
    map.insert("p2".into(), "mainline".into());
    assert!(project_role(&map, "p1").is_none());
    assert_eq!(project_role(&map, "p2"), Some(ListRole::Mainline));
}

#[test]
fn sync_with_fake_http_writes_cache() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    struct Fake;
    impl TickTickHttp for Fake {
        fn get_json(&self, url: &str, _b: Option<&str>) -> Result<String, String> {
            if url.ends_with("/open/v1/project") {
                return Ok(r#"[{"id":"p2","name":"科研"}]"#.into());
            }
            Ok(r#"{"tasks":[{"id":"1","title":"HDP","status":0,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"}]}"#.into());
        }
        fn post_form(&self, _u: &str, _f: &[(&str, &str)]) -> Result<String, String> {
            Err("no write".into())
        }
    }
    let mut roles = BTreeMap::new();
    roles.insert("p2".into(), "mainline".into());
    let n = sync_projects(&Fake, &conn, &roles, "tok", 1_000).unwrap();
    assert_eq!(n, 1);
    let rows = load_ticktick_cache(&conn).unwrap();
    assert_eq!(rows[0].id, "tt-1");
}
```

RFC 7636 附录的 verifier/challenge 对必须一字不差。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- ticktick::tests::pkce_challenge_is_s256_unpadded`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

加 sha2。实现 PKCE、缓存、`sync_projects`、命令。`finish_oauth` 测 `parse_token_response`：

```json
{"access_token":"a","refresh_token":"r","token_type":"bearer","expires_in":3600}
```

命令在 `lib.rs` `generate_handler!` 注册。`api.ts` 增加对应 invoke。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- ticktick::`

Expected: PASS。`cargo test --offline -p gamelife` 必须整包绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/ticktick.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: sync TickTick projects read-only into local cache

OAuth PKCE and a fake HTTP trait keep tokens off disk
and keep tests offline.
EOF
)"
```

---

### Task 12: 槽开始钉 TickTick 缓存快照

**Files:**
- Modify: `src-tauri/src/scheduler.rs`（`pin_task_snapshot_json`）
- Modify: `src-tauri/src/commands.rs`（`save_settings` 写入 `admin_apps` 与 `category_guides`；`get_today` 可选触发 sync）

**Interfaces:**
- Consumes: `load_ticktick_cache`、`ticktick_judgment_set`、`snapshot` 结构 `TaskSnapshot { id, title, role }`
- Produces: `pin_task_snapshot_json(conn, day)` **不再读本机 `tasks` 表**。从缓存筛当天相交时段，`>20` 已由 `ticktick_judgment_set` 变空。`serde_json::to_string` 写入 `task_snapshot_json`。

槽开始若缓存不 fresh：调用 `ticktick_sync`，超时失败则用旧缓存；无缓存则 `"[]"`。测试里直接插入 `ticktick_cache` 再 `ensure_slot`/`pin`。

`save_settings` 的政策 JSON 必须含：

```json
trusted_apps, distraction_rules, side_project_rules, reading_apps, never_capture_apps, admin_apps, category_guides
```

`load_policy` / `PolicyJson` 增加 `admin_apps`、`category_guides` 的 Option 默认。

- [ ] **Step 1: Write the failing test**

在 `scheduler.rs` tests：

```rust
#[test]
fn pin_snapshot_uses_ticktick_cache_not_local_tasks() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    // 插入本机 tasks 一行「不该出现」
    conn.execute(
        "INSERT INTO task_lists (id, name, sort, role) VALUES ('list-mainline','主线任务',0,'mainline')",
        [],
    ).ok();
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end) VALUES ('task-local','list-mainline','LOCAL',0,1000,1900)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at) VALUES ('tt-9','p','RAIDS+', 'mainline', 1000, 1900, 50)",
        [],
    ).unwrap();
    let day = "2026-09-13";
    // 用与 1000 同日的 day 字符串：按 Local 构造。测试应计算 day_str_for_ts(1000) 或把 start 换成 start_of_named_day。
    let json = pin_task_snapshot_json(&conn, &day_str_for_ts(1_778_083_200));
    assert!(json.contains("tt-9"), "{json}");
    assert!(!json.contains("LOCAL"), "{json}");
}
```

选一个稳定时间戳：用 `start_of_named_day` 测。把 cache 的 start/end 设在该日 10:00–11:00。

`save_settings` 测：内存 DB + 写政策后 `load_policy` 看到 `admin_apps`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- scheduler::tests::pin_snapshot_uses_ticktick_cache_not_local_tasks`

Expected: FAIL（仍钉本地 LOCAL）

- [ ] **Step 3: Write minimal implementation**

重写 `pin_task_snapshot_json`。更新 `PolicyJson` 与 `load_policy` / `load_policy_for_version`。`save_settings` 写入新字段。

槽开始刷新：在 `ensure_slot` 钉快照前，若 `ticktick_status.connected` 且不 fresh，best-effort `sync`（错误只 eprintln）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- scheduler::tests::pin_snapshot_uses_ticktick_cache_not_local_tasks`

Expected: PASS。`cargo test --offline -p gamelife` 全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: pin TickTick cache as the slot judgment snapshot

Local task rows remain in SQLite but no longer decide
what the judge sees.
EOF
)"
```

---

### Task 13: 文本 AI prompt 与灰区双路径

**Files:**
- Modify: `src-tauri/src/text_ai.rs`
- Modify: `src-tauri/src/scheduler.rs`（finalize 灰区）

**Interfaces:**
- Consumes: `CategoryGuides`、`nonempty_guides`、`parse_task_match_json`、`parse_category_match_json`、`apply_task_match`、`apply_category_match`
- Produces:

```rust
pub fn build_text_ai_prompt(
    snapshots: &[TaskSnapshot],
    guides: &CategoryGuides,
    policy: &Policy,
    sample_summary: &str,
) -> String
```

Prompt 规则：

- 保护样本已在 `sample_summary_lines` 过滤
- 有快照：现有「Match ... task_id」+ Tasks 列表 + Windows + 非空 guides + 名单摘要（每类最多 20 个名字）
- 无快照：要求 `{"category": string|null, "confidence": number}` + guides + 名单 + Windows
- 空 guides 字符串不得出现

`call_text_task_match` 改为接收完整 prompt 或拆 `call_text_json(endpoint, prompt)`。

Finalize：`gray && !summary.is_empty()` 时打 AI。有快照走 task_id；无快照走 category。`matched_text` 成功则不打视觉。娱乐硬规则已决议的槽不是 gray，不调用 AI。

名单摘要：

```rust
pub fn policy_names_blurb(policy: &Policy) -> String
```

格式固定中文键：`主线应用: ...\n支线: ...\n杂项: ...\n娱乐: ...`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn prompt_omits_empty_guides_and_protected_lines_already_filtered() {
    let p = default_v01();
    let prompt = build_text_ai_prompt(&[], &CategoryGuides::default(), &p, "app=Cursor title=x url= document_path= idle=1");
    assert!(!prompt.contains("主线："));
    assert!(prompt.contains("category"));
    assert!(!prompt.contains("task_id"));
}

#[test]
fn prompt_with_snapshot_asks_for_task_id() {
    let snaps = [TaskSnapshot { id: "tt-1".into(), title: "HDP".into(), role: ListRole::Mainline }];
    let mut g = CategoryGuides::default();
    g.mainline = "写论文".into();
    let prompt = build_text_ai_prompt(&snaps, &g, &default_v01(), "app=Cursor title=HDP url= document_path=/p/HDP/a.py idle=1");
    assert!(prompt.contains("task_id"));
    assert!(prompt.contains("写论文"));
    assert!(prompt.contains("tt-1"));
}
```

Scheduler：用假的不联网路径——可单测 `build_text_ai_prompt` 足够；再加一个 finalize 单测较重。至少加：

娱乐 dominant 时不要求 prompt（在 `text_ai` 测 `sample_summary_lines` 过滤 secure_input 已存在则补 protected 行测试）。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- text_ai::tests::prompt_omits_empty_guides_and_protected_lines_already_filtered`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 prompt；改 finalize 灰区。无密钥 / 解析失败 → 保持 gray 走视觉。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- text_ai::`

Expected: PASS。`cargo test --offline -p gamelife` 全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/text_ai.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: send category guides to text AI on gray slots

Empty snapshots classify by category; timed snapshots
still match a single task id.
EOF
)"
```

---

### Task 14: 槽决议时写入 App/host 日汇总

**Files:**
- Create: `crates/gamelife-core/src/app_stats.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Modify: `src-tauri/src/resolve.rs`

**Interfaces:**
- Consumes: `Sample`、`Hint`、`url_host`、`is_protected` 判定（secure_input 或 never-capture 应用）
- Produces:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppHintSecs { /* 与 app_day_stats 数值列同名 */ }

pub fn hint_bucket(hint: Hint, protected: bool) -> &'static str
// protected → "protected"；Unobserved 不在 hint 里
// Admin→admin, Side→side, Distraction→distraction, Away→away
// CoreCandidate/CoreReading→core, Unsure/UnsureReading→"" （只 samples+1）

pub fn accumulate_sample(
    app: &mut AppHintSecs,
    hint: Hint,
    protected: bool,
    idle: i64,
    secs: i64, // 15
)
```

`resolve_slot` 在 `upsert_slot` 成功且非 pending 以及 pending（spec：决议时写入；pending 也是槽结束）—— **pending 与 final 都写**，用该槽 `analyze_slot_evidence` 的 per-sample hint。Resolve 当前没有 samples。需要 `resolve_slot` 增加参数 `stats: &[AppStatInc]` 或在 scheduler finalize 在调用 resolve 前算好 increments，resolve 接收并 INSERT。

为少改签名，推荐：

```rust
pub struct DayStatDelta {
    pub app: String,
    pub bundle_id: String,
    pub host: Option<String>,
    pub samples: i64,
    pub idle_seconds: i64,
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
    pub protected: i64,
}

pub fn deltas_from_slot(
    day_samples: &[Sample],
    hints: &[Hint],
    never: &[String],
) -> Vec<DayStatDelta>
```

`resolve_slot(..., deltas: &[DayStatDelta])`。现有 resolve 测试传 `&[]`。同一 `tx`：

```sql
INSERT INTO app_day_stats (...) VALUES (...)
ON CONFLICT(day, app, bundle_id) DO UPDATE SET
  samples = samples + excluded.samples,
  ...
```

host 非空另写 `host_day_stats`。`unobserved` 来自缺口 span 而不是单样本——spec 说「每条样本 +15s 记入对应类别」。缺口 unobserved **不进 app 表**（没有 app）。保护样本：`protected += 15`，其它类别列不加。

Scheduler finalize 在 `resolve_slot` 前：`deltas_from_slot(&samples, &evidence.hints, &never)`。

- [ ] **Step 1: Write the failing test**

core：

```rust
#[test]
fn protected_sample_only_adds_protected_seconds() {
    let mut a = AppHintSecs::default();
    accumulate_sample(&mut a, Hint::Unsure, true, 0, 15);
    assert_eq!(a.protected, 15);
    assert_eq!(a.core, 0);
    assert_eq!(a.samples, 1);
}
```

resolve：内存 DB migrate，造 samples+hints 经 finalize 或直接 `resolve_slot` + deltas，查询 `app_day_stats.core = 15`。

失败时槽必须仍非 final：在 insert_ledger 前先写 stats，stats SQL 失败 return Err。单测可用错误 app 名超长——不必。至少断言事务内两表都有行。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- app_stats::tests::protected_sample_only_adds_protected_seconds`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 deltas；改 `resolve_slot` 与所有测试调用补 `&[]` 或真实 deltas。finalize 传入真实 deltas。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- app_stats`

再：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife -- resolve::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/app_stats.rs crates/gamelife-core/src/lib.rs src-tauri/src/resolve.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: roll hint seconds into daily app stats on resolve

Protected samples are stored separately so never-capture
time cannot look like entertainment or core.
EOF
)"
```

---

### Task 15: 报表纯函数与 Tauri 命令

**Files:**
- Create: `crates/gamelife-core/src/reports.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: `ActivitySeconds`、`sum_activity`、slots 行、ledger、redemptions、`freeze_uses`、`app_day_stats`
- Produces:

```rust
pub fn hit_rate(credited_secs: &[i64], threshold: i64) -> f64
// 分母为切片长度，空切片 → 0.0

pub fn wow_delta(this_week_core: i64, last_week_core: i64) -> Option<i64>
// last 无数据 None

pub fn distraction_runs(slot_is_distraction: &[bool]) -> (usize, i64)
// 连续 true ≥3 的段数；总 true*900 由调用方用槽长。函数返回 (run_count, run_slots)
// run 定义为连续 ≥3 个 true

pub fn first_core_hour(slot_starts: &[i64], day_start: i64) -> Option<i32>
// 当天第一个 credited_core>0 的槽开始的本地小时。core 模块用秒偏移：((slot_start - day_start) / 3600) as i32

pub fn month_heat_cell(credited_core: i64) -> f64
// credited_core / 28800 clamp 0..=1
```

命令：

- `get_week` **扩展字段**（旧字段保留，商店不破）：

```rust
pub core_hours: f64,            // credited 主线秒/3600 一位小数在前端 format 也可；后端给 i64 分钟更干净
pub wow_core_delta_minutes: Option<i64>,
pub distraction_observed_ratio: f64, // distraction / max(observed,1) 用周 activity
pub pending_over_resolved: f64,
pub days_ge_6h: i64,
pub days_ge_8h: i64,
```

周 `observed` = 各类 activity 之和 − unobserved？Spec：娱乐占观测比 = distraction / (total activity − unobserved)。分母 0 则 0.0。

- `get_month_report(year: i32, month: i32) -> MonthReportView`
- `get_rhythm_report(kind: String, anchor: String) -> RhythmReportView`
- `get_app_report(kind: String, anchor: String) -> AppReportView`

`kind` 为 `"week"` | `"month"`。`anchor` 为 `YYYY-MM-DD`。

`TodayView` 增加 `app_top: Vec<AppTopRow>`（最多 5）与 `pending_count: i64`。`get_today` 从 `app_day_stats` 填。

`MonthReportView`：`days: Vec<{ day, creditedCore, isWeekend, isFuture }>`，`activity: ActivitySeconds` 的分钟字段，`coinsEarned`，`coinsSpent`，`xpEarned`，`goldDays`，`freezeCount`，`completedDays`。

`RhythmReportView`：`startHours: Vec<{ day, hour: number | null }>`，`rate6h`，`rate8h`，`distractionRunCount`，`distractionRunSlots`，`peakHours: Vec<i32>`（最多 3，来自 by_hour 主线/观测）。

`AppReportView`：`apps: Vec<{ name, minutes, dominant, listedAs }>`，`newcomers: Vec<String>`，`hosts: Vec<{ host, minutes, dominant }>`，`protectedMinutes`。

`listedAs`：对照当前 `load_policy` 四名单 + 阅读 + 永不截屏，无匹配则空字符串（进 newcomers）。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn mixed_slot_minutes_do_not_become_fifteen_core_in_hit_rate_denom() {
    assert_eq!(hit_rate(&[480, 0], 3600), 0.0);
    assert!((hit_rate(&[6 * 3600, 8 * 3600], 6 * 3600) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn distraction_run_needs_three_slots() {
    assert_eq!(distraction_runs(&[true, true, false]), (0, 0));
    assert_eq!(distraction_runs(&[true, true, true, false, true, true, true]), (2, 6));
}

#[test]
fn month_heat_caps_at_one() {
    assert_eq!(month_heat_cell(0), 0.0);
    assert_eq!(month_heat_cell(28800), 1.0);
    assert_eq!(month_heat_cell(40000), 1.0);
}
```

commands 测：migrate 内存库，插入两个槽 activity 8m core + 7m side，`get_week`/`build_week` 两类都非 0（现有 week 测试若已有则加断言 pending_over_resolved 分母）。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- reports::tests::distraction_run_needs_three_slots`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 reports；扩展 `WeekView`；实现三个 get_* 与 today 新字段。注册命令。更新 `api.ts`。

周环比：`anchor` 本周 core 分钟 − 上一 ISO 周。无上一周槽则 `None`。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core -- reports`

再：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/reports.rs crates/gamelife-core/src/lib.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: expose week, month, rhythm, and app report commands

Aggregations stay on activity seconds so an 8+7 slot
cannot become fifteen minutes of core.
EOF
)"
```

---

### Task 16: 设置页 — 名单、一句话样稿、TickTick

**Files:**
- Create: `src/lib/guides.ts`
- Create: `src/lib/guides.test.ts`
- Modify: `src/pages/Settings.tsx`
- Modify: `src/lib/api.ts`（TickTick invoke 若 Task 11 已加则接 UI）

**Interfaces:**
- Consumes: `AppSettings`、TickTick 命令
- Produces: `GUIDE_PLACEHOLDERS` 与 spec 原文一致：

```ts
export const GUIDE_PLACEHOLDERS = {
  mainline: "当天 TickTick 主线清单里的科研任务，以及在 Cursor、论文 PDF、Overleaf 上写代码、改稿、看文献。",
  side: "与当天主线课题无直接关系的工具、个人项目、整理仓库、打磨 GameLife。",
  admin: "邮件、报销、填表、组会行政、改个人主页。",
  entertainment: "视频、社交媒体、购物、无目的刷网。B 站和 YouTube 默认算娱乐。",
} as const;
```

设置标签：基础 / API / 名单 / TickTick。基础增加勾选「导航显示文字」。名单：主线应用（原信任应用）、支线应用、杂项应用、娱乐应用 / 网站、阅读、永不截屏。其下四 textarea，`placeholder={GUIDE_PLACEHOLDERS.*}`，`value` 为已保存正文（空则显示 placeholder 不把样稿当作值）。保存时 `categoryGuides` 为 trim 后的实际输入，**不得**把 placeholder 写进 Policy。

TickTick 板块：Client ID 输入；Client Secret 密码框（保存调 `ticktick_set_client_secret`）；连接按钮走 `ticktick_begin_oauth`（`window.open` authorizeUrl 或提示把回调贴回——实现：`begin` 返回 url，前端 `window.open`，同时 Rust 已在 listen loopback；`finish` 由 Rust 线程自己完成更顺。若 UI 线程阻塞，begin 在后台线程 listen 并 `finish` 写钥匙串，前端轮询 `ticktick_status`）。

单测不测 OAuth 弹窗。列表映射：拉取 `ticktick_list_projects`，每行 select：忽略/主线/支线/长期/杂项，写入 `ticktickProjectRoles` 后 `saveSettings`。手动「同步任务」。未连接红字。超过 20 条的说明用一段 muted：「当天有时段任务超过 20，请在 TickTick 勾完或改期。」仅当 `ticktick_sync` 返回码或字段 `truncated: true` 时显示——Task 11 `sync` 返回 `{ count, truncated: bool }`。

回到 Task 11 若返回值只是 count：本任务把 `ticktick_sync` 改为返回 `{ count: number, truncated: boolean }`（`ticktick_judgment_set` 空且缓存行≥21 则为 true）。若 Task 11 已合并则此处改命令返回值并补 Rust 测试。

- [ ] **Step 1: Write the failing test**

```ts
import { GUIDE_PLACEHOLDERS } from "./guides";
describe("GUIDE_PLACEHOLDERS", () => {
  it("matches spec copy", () => {
    expect(GUIDE_PLACEHOLDERS.mainline).toContain("Overleaf");
    expect(GUIDE_PLACEHOLDERS.entertainment).toContain("YouTube");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/guides`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

加 `guides.ts`、改 Settings 四板块、基础开关、TickTick UI。保存路径包含 `adminApps`、`categoryGuides`、`showRailLabels`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/guides.ts src/lib/guides.test.ts src/pages/Settings.tsx src/lib/api.ts src-tauri/src/commands.rs src-tauri/src/ticktick.rs
git commit -m "$(cat <<'EOF'
feat: add category guide placeholders and TickTick settings

Placeholders stay visual-only so an empty box does not
silently become policy text.
EOF
)"
```

---

### Task 17: 侧栏图标与是否显示小字

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src/lib/api.ts`（若需 `getSettings` 读 `showRailLabels`）

**Interfaces:**
- Consumes: `getSettings().showRailLabels`（启动拉一次，设置保存后若在设置页改开关，返回 App 时再拉——`App` 在 tab 切到 settings 以外时 refresh，或 `window` 自定义事件。最简：`App` 每 5s 已 poll `getToday`，改为同时记住 labels 则不够。**App mount + 每次切 tab 时 `getSettings` 只取 `showRailLabels`。**）
- Produces: 四个内联 SVG（今日日历、统计柱、商店袋、齿轮），`aria-label` 中文。`showRailLabels===false` 时不渲染 `.rail-label`。tab `week` 的可见标签改为「统计」。

- [ ] **Step 1: Write the failing test**

无现成 App 渲染测试。抽：

```ts
// src/lib/rail.ts
export function railTabs(): { id: "today"|"week"|"shop"|"settings"; label: string }[] {
  return [
    { id: "today", label: "今日" },
    { id: "week", label: "统计" },
    { id: "shop", label: "商店" },
    { id: "settings", label: "设置" },
  ];
}
```

```ts
expect(railTabs()[1].label).toBe("统计");
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/rail`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

`rail.ts` + `App.tsx` 用 SVG 替换「今/周/店/设」。CSS：无 label 时按钮 padding 仍可点。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/rail`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/lib/rail.ts src/lib/rail.test.ts src/styles.css
git commit -m "$(cat <<'EOF'
feat: use icon rail with optional labels

Stats replaces the Week caption so the monitor shell
no longer reads as a planner.
EOF
)"
```

---

### Task 18: 今日页改为日报 + 单列复核轴

**Files:**
- Modify: `src/pages/Today.tsx`
- Modify: `src/lib/calendar.ts`（若需计划竖线）
- Modify: `src/lib/calendar.test.ts`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `getToday`、`getDayView`、`getAppReport` 不必（用 `today.appTop`）、`reviewSlot`、`freezeDay`、`endToday`
- Produces: 删除 NL 表单、列表分组、新建列表、计划|实际双列、Progress32 两行网格、徽章两行布局。保留权限条、娱乐条、结束今天。

`.today-shell` 在窗口高度内自己滚动（`flex:1; min-height:0; overflow:auto`），不要跟侧栏一起滚掉底部圆角。

结构：

1. 一行 `.today-badges.today-badges-row`：硬币、能量、连胜
2. 一条主线进度：`credited / 8h`，旁注宝箱与黄金日 `have/need`（数据仍来自 `chest`/`gold`，只是不再单独两行网格）
3. 类别分钟条（用 `slots` 的 activity 汇总；若 today 无聚合则前端对 `dayView.slots` 求和——**禁止 dominant×15**。优先用新字段 `today.activityMinutes`；若 Task 15 未加，本任务在 `get_today` 增加 `activity: SlotActivityMinutes` 日合计，来自当天 slots `sum_activity`）
4. 时间轴：96 槽一列实际块，当前槽「正在识别」，红线现在。TickTick 计划：用 `dayView.tasks` **不再**；用 `dayView.planMarks: { start, end, title }[]`。本任务在 `get_day_view` 从当天 `task_snapshot_json` **并集**或从 `ticktick_cache` 筛该日，画短竖线。点实际块打开现有 `SlotReview`
5. 当日 Top 5 App
6. 冻结：`<details>` 默认折叠，标题「保护连胜」
7. 结束今天

徽章不随 `calDay` 改变。类别条与时间轴随 `calDay`。

`calendar.ts`：

```ts
export function planMarksFromSnapshots(
  tasks: { start: number | null; end: number | null; title: string }[],
  dayStart: number,
): { start: number; end: number; title: string }[]
```

只保留与当天相交且有起止的。

- [ ] **Step 1: Write the failing test**

```ts
it("keeps overlapping timed marks only", () => {
  const dayStart = Date.UTC(2026, 8, 13) / 1000;
  const marks = planMarksFromSnapshots(
    [
      { start: dayStart + 10 * 3600, end: dayStart + 11 * 3600, title: "A" },
      { start: null, end: null, title: "B" },
    ],
    dayStart,
  );
  expect(marks).toHaveLength(1);
  expect(marks[0].title).toBe("A");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/calendar`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 marks；重写 `Today.tsx`；CSS 一行徽章、单列轴、`details` 冻结。确认文件中无「新建列表」「添加任务」字符串。

`get_today` / `get_day_view` 补 `activity` 合计与 `planMarks`（Rust 侧小结构）。补 commands 测试：overlapping 任务进 marks。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

再：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/pages/Today.tsx src/lib/calendar.ts src/lib/calendar.test.ts src/styles.css src/lib/api.ts src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: replace the planner Today page with a daily report

Review stays on a read-only actual timeline; planning
ticks are marks, not a second drag calendar.
EOF
)"
```

---

### Task 19: 统计页 — 周 / 月 / 节奏 / 应用

**Files:**
- Modify: `src/pages/Week.tsx`（可改名为 `Stats.tsx` 并改 App import；若改名必须同时改 import）
- Modify: `src/styles.css`
- Modify: `src/lib/api.ts`
- Create: `src/lib/monthGrid.ts` + `src/lib/monthGrid.test.ts`

**Interfaces:**
- Consumes: `getWeek`、`get_month_report`、`get_rhythm_report`、`get_app_report`
- Produces: 顶栏分段「周 / 月 / 节奏 / 应用」+ 范围 ‹ ›。默认周。节奏/应用旁「按周 | 按月」。

周：原三块升级（类别加占观测比；按天娱乐不进主线堆叠，另细线或第二图例；热力 8–21）。下方大数：主线小时、环比、娱乐占比、待复核率、≥6h 日数、≥8h 日数。空状态「本周还没有观测」。

月：热力月历用 `month_heat_cell` 色；周末/未来不同 class。点格：`onPickDay(day)` 切 tab 今日——`App` 需能受控。实现：`localStorage.setItem("gl-cal-day", day)` 并 `setTab("today")`；Today 读这个 key 设 `calDay` 后删 key。不要把连胜徽章改成该日。

节奏、应用：按 spec 四块/四表。新面孔置顶。保护分钟单独一行。无 `app_day_stats` 时「无应用明细」。

`monthGrid.ts`：

```ts
export function calendarCells(year: number, month: number): { day: string | null; weekday: number }[]
// month 1-12；前导空格 day=null
```

- [ ] **Step 1: Write the failing test**

```ts
it("pads September 2026 leading blanks to Tuesday-start? compute from Date", () => {
  const cells = calendarCells(2026, 9);
  expect(cells.length % 7).toBe(0);
  expect(cells.filter((c) => c.day === "2026-09-13").length).toBe(1);
});
```

2026-09-13 是周日（用户环境）。测试只断言该日存在且网格是 7 的倍数。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/monthGrid`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现月历格子与统计页四段。类别分钟来自 week/month activity 秒 / 60 取整。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/pages/Week.tsx src/pages/Stats.tsx src/App.tsx src/lib/monthGrid.ts src/lib/monthGrid.test.ts src/styles.css src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: expand Week into week, month, rhythm, and app stats

Weekend cells stay unsampled rather than fake zero core.
EOF
)"
```

若未改名为 Stats.tsx，不要把不存在的文件加入 `git add`。

---

### Task 20: 商店欲望视觉

**Files:**
- Create: `src/lib/shopUnlock.ts`
- Create: `src/lib/shopUnlock.test.ts`
- Modify: `src/pages/Shop.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: 现有 `getWeek`、`redeem`、`createWish`、`updateWish`、`archiveWish`、`splitWishes`、错误码
- Produces:

```ts
export function minutesUntilShopUnlock(creditedTodayMinutes: number): number {
  return Math.max(0, 60 - creditedTodayMinutes);
}
```

页面：英雄区三列（硬币余额、今日能量或进行中剩余、解锁进度 60 分钟）。进行中大卡仅当 `activeEntertainment` 存在。去掉商店页 `EntertainmentBanner`。货架 `minmax(160px, 1fr)`。锁定按钮文案 `差 ${n} 分钟`。记录最近 30 条，进行中左边框。错误文案表按 spec H §7。种子与兑换事务不改。

- [ ] **Step 1: Write the failing test**

```ts
expect(minutesUntilShopUnlock(10)).toBe(50);
expect(minutesUntilShopUnlock(60)).toBe(0);
expect(minutesUntilShopUnlock(90)).toBe(0);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/shopUnlock`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 helper 与 Shop 布局/CSS。卡片 glyph：名称首字或简单 SVG。对比度：disabled 不用 0.2 opacity。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/shopUnlock.ts src/lib/shopUnlock.test.ts src/pages/Shop.tsx src/styles.css
git commit -m "$(cat <<'EOF'
feat: make the shop feel like a reward shelf

Unlock progress and a live entertainment card stay
cosmetic; redeem still uses the existing transaction.
EOF
)"
```

---

### Task 21: 全量回归与 UI 禁令扫描

**Files:**
- 不新增功能。必要时修编译与测试。
- Modify: 仅当回归失败的文件

**Interfaces:**
- Consumes: 全仓库
- Produces: 下列命令全绿；`rg` 确认今日页无计划本文案

- [ ] **Step 1: Write the failing test**

在 `src/pages/Today.tsx` 相关的 grep 测试不必上线。执行人工扫描步骤（下一步命令即测试）。

- [ ] **Step 2: Run the full suite（失败则修，视为本任务实现）**

Run:

```bash
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife-core
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target cargo test --offline -p gamelife
npx vitest run --dir src
rg -n "新建列表|自然语言|计划\|实际|FullCalendar|dnd-kit" src/pages/Today.tsx src/App.tsx src/pages/Week.tsx || true
```

Expected: 两个 crate 与 vitest PASS。`Today.tsx` 无「新建列表」、无 nl-form。允许 `rg` 在计划文档命中。

- [ ] **Step 3: Write minimal implementation**

只修回归失败处：Policy 字面量、`resolve_slot` 新参数、`WeekView` 前端解构、Hint match 非穷尽。禁止顺手加功能。

- [ ] **Step 4: Run test to verify it passes**

重复 Step 2，全部 PASS。

- [ ] **Step 5: Commit**

若有修复：

```bash
git add -u
git commit -m "$(cat <<'EOF'
fix: land monitor-shell regressions after the report UI

Keep judge, ledger, and shop invariants green together.
EOF
)"
```

若工作树干净则不提交。

---

## 执行说明

任务 1–7 只动 `gamelife-core`（Task 6 例外，动 scheduler）。8–15 动 `src-tauri`。16–20 动前端但依赖已注册的命令。不要并行改 `judge_slot` 签名两次。统计与商店 UI 可在 Task 15 命令合并后并行，但仍须按本文件顺序勾选，便于单一执行会话。

禁止把周报写入 TickTick、禁止完成 TickTick 任务来兑换、禁止恢复今日页加任务。
