# B Grounded Core Judgment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 取消空闲→Away；Quest 可对 URL；H2 科研 host 算落地证据；自动 Core 只统计落地活跃秒；D2 进入默认 distraction，并用 policy seed v2 补到旧库。

**Architecture:** 不拆 `Hint`。`url_host` / `host_is_research` 提供 host 判定；`is_grounded_core_sample` 判断路径/URL/H2；`hint_sample` 只改命中与空闲；`SlotEvidence.grounded_strong_core_seconds` 驱动自动 Core 与 `metadata_decidable`。

**Tech Stack:** Rust、`gamelife-core`、rusqlite policy seed。不新增 URL crate。

**Spec:** `docs/superpowers/specs/2026-09-11-judgment-grounded-core-design.md`

## Global Constraints

- 实现做在 `feat/a1-observation-engine`（A1 已落地的 `document_path` / `url`）。不要在落后的 `main` 上改判定。
- 不改观测引擎、15 秒槽、截图调度、账本、Quest / 商店、视觉 prompt、`verified_core` 窗口族、阅读应用列表。
- `Hint` 枚举不增加变体。
- 不得从标题伪造 `document_path`。判定不得要求「一定有 URL」。
- Quest 为空 → credited=0（即使 H2 落地）。
- 自动 Core：`grounded_strong_core ≥ 780` 且 `side+distraction ≤ 60`；该分支 credited = `grounded_strong_core + reading_bridge`。
- 灰区视觉/人工 credited 仍用 `strong_core + reading_bridge + verified_core`。
- Distraction 先于 Side、先于 Core。
- 科研 host 是代码常量，不进 Policy JSON。
- `policy_seed_version=2` 之后清空的 distraction 不再补回。
- 测试禁止 `std::env::set_var("HOME", …)`。不要改无关测试里已有的 `set_var`。
- 改 `crates/gamelife-core`：`cargo test --offline -p gamelife-core` 必须绿。
- 改 `src-tauri/`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife` 必须编译并跑过。
- 不做 Chrome/Safari/Arc 阅读应用，不把 Arc 补进 Trusted，不把 GitHub/ChatGPT 进科研白名单。

---

## File Structure

```
crates/gamelife-core/src/url.rs             url_host、host_is_research
crates/gamelife-core/src/policy.rs           default_distraction_rules、default_v01
crates/gamelife-core/src/hint.rs            is_grounded_core_sample、hint_sample
crates/gamelife-core/src/judge.rs           SlotEvidence.grounded_strong_core_seconds、自动 Core
crates/gamelife-core/src/lib.rs             再导出新函数
src-tauri/src/scheduler.rs                  metadata_decidable、seed v2
docs/superpowers/specs/2026-09-11-judgment-grounded-core-design.md
```

---

### Task 1: `url_host` + `host_is_research`

**Files:**
- Modify: `crates/gamelife-core/src/url.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（`pub use url::{host_is_research, strip_url_query_fragment, url_host};`）

**Interfaces:**
- Consumes: 现有 `strip_url_query_fragment`
- Produces:

```rust
pub fn url_host(url: &str) -> Option<String>
pub fn host_is_research(host: &str) -> bool
```

只接受 `http://` / `https://`。host 小写、去端口、去 userinfo。禁止对整段 URL 做 `nature` 子串匹配。

H2 基名用「等于或后缀 `.base`」：`overleaf.com`、`arxiv.org`、`ieee.org`、`acm.org`、`nature.com`、`sciencedirect.com`、`webofscience.com`、`pubmed.ncbi.nlm.nih.gov`。

Scholar：host（可去掉前缀 `www.`）等于 `scholar.google.com`，或 `scholar.google.` 之后为 `[a-z]{2,3}`，可选再加 `.` + `[a-z]{2}`。`scholar.google.com.evil.example` 为假。

- [ ] **Step 1: 写失败测试**

在 `url.rs` 的 `tests` 里追加（保留现有 `strip_query_and_hash`）：

```rust
    use super::{host_is_research, url_host};

    #[test]
    fn arxiv_host_is_research() {
        assert_eq!(
            url_host("https://arxiv.org/abs/1?x=2").as_deref(),
            Some("arxiv.org")
        );
        assert!(host_is_research("arxiv.org"));
    }

    #[test]
    fn research_hosts_and_suffixes() {
        for url in [
            "https://www.overleaf.com/project/abc",
            "https://ieeexplore.ieee.org/document/1",
            "https://scholar.google.com/scholar?q=a",
            "https://scholar.google.co.uk/scholar",
            "https://dl.acm.org/doi/10.1",
            "https://www.nature.com/articles/s1",
            "https://www.sciencedirect.com/science/article/pii/x",
            "https://www.webofscience.com/wos",
            "https://pubmed.ncbi.nlm.nih.gov/123",
        ] {
            let host = url_host(url).unwrap_or_else(|| panic!("host for {url}"));
            assert!(host_is_research(&host), "{url} host={host}");
        }
    }

    #[test]
    fn github_nature_and_youtube_are_not_research() {
        assert_eq!(
            url_host("https://github.com/nature/foo").as_deref(),
            Some("github.com")
        );
        assert!(!host_is_research("github.com"));
        assert!(!host_is_research(&url_host("https://www.youtube.com/watch?v=1").unwrap()));
        assert_eq!(url_host(""), None);
        assert_eq!(url_host("not a url"), None);
        assert!(!host_is_research("scholar.google.com.evil.example"));
    }
```

`url_host` / `host_is_research` 先不要实现（或 `unimplemented!()`），让测试失败。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- url::tests::github_nature_and_youtube_are_not_research`

Expected: FAIL（未实现或断言失败）

- [ ] **Step 3: 最小实现**

```rust
fn is_alpha_len(s: &str, min: usize, max: usize) -> bool {
    let n = s.len();
    n >= min && n <= max && s.bytes().all(|b| b.is_ascii_lowercase())
}

fn host_matches_base(host: &str, base: &str) -> bool {
    host == base || host.ends_with(&format!(".{base}"))
}

fn host_is_scholar(host: &str) -> bool {
    let h = host.strip_prefix("www.").unwrap_or(host);
    if h == "scholar.google.com" {
        return true;
    }
    let Some(rest) = h.strip_prefix("scholar.google.") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    match parts.as_slice() {
        [a] if is_alpha_len(a, 2, 3) => true,
        [a, b] if is_alpha_len(a, 2, 3) && is_alpha_len(b, 2, 2) => true,
        _ => false,
    }
}

pub fn url_host(url: &str) -> Option<String> {
    let stripped = strip_url_query_fragment(url);
    let lower = stripped.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("https://") {
        r
    } else if let Some(r) = lower.strip_prefix("http://") {
        r
    } else {
        return None;
    };
    let hostport = rest.split('/').next().unwrap_or("").trim();
    if hostport.is_empty() {
        return None;
    }
    let hostport = hostport.rsplit('@').next()?.trim();
    let host = if let Some(rest) = hostport.strip_prefix('[') {
        let end = rest.find(']')?;
        rest[..end].to_string()
    } else {
        hostport.split(':').next()?.to_string()
    };
    let host = host.trim_end_matches('.').to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

const RESEARCH_BASES: &[&str] = &[
    "overleaf.com",
    "arxiv.org",
    "ieee.org",
    "acm.org",
    "nature.com",
    "sciencedirect.com",
    "webofscience.com",
    "pubmed.ncbi.nlm.nih.gov",
];

pub fn host_is_research(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    host_is_scholar(&host) || RESEARCH_BASES.iter().copied().any(|b| host_matches_base(&host, b))
}
```

`lib.rs` 把 `pub use url::strip_url_query_fragment` 改成同时导出 `url_host`、`host_is_research`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core -- url`

Expected: PASS。再跑：`cargo test --offline -p gamelife-core`

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/url.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: match research hosts by DNS suffix not URL substring

Keep GitHub paths containing "nature" out of the research
allowlist so auto-core can key off Overleaf and arXiv hosts.
EOF
)"
```

---

### Task 2: `default_distraction_rules`

**Files:**
- Modify: `crates/gamelife-core/src/policy.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（`pub use policy::{..., default_distraction_rules, ...}`）

**Interfaces:**
- Consumes: 无
- Produces:

```rust
pub fn default_distraction_rules() -> Vec<String>
```

固定顺序：`bilibili.com`、`youtube.com`、`twitter.com`、`x.com`、`douyin.com`、`tiktok.com`。

`default_v01()` 的 `distraction_rules` 改为 `default_distraction_rules()`，不要再写 `vec![]`。

- [ ] **Step 1: 写失败测试**

在 `policy.rs` `tests` 追加：

```rust
    #[test]
    fn default_v01_includes_d2_distraction_hosts() {
        let p = default_v01();
        for host in [
            "bilibili.com",
            "youtube.com",
            "twitter.com",
            "x.com",
            "douyin.com",
            "tiktok.com",
        ] {
            assert!(
                p.distraction_rules.iter().any(|r| r == host),
                "missing {host}"
            );
        }
        assert_eq!(p.distraction_rules, default_distraction_rules());
    }
```

`default_distraction_rules` 先不存在或 `default_v01` 仍返回空列表，使测试失败。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- policy::tests::default_v01_includes_d2_distraction_hosts`

Expected: FAIL

- [ ] **Step 3: 最小实现**

```rust
pub fn default_distraction_rules() -> Vec<String> {
    vec![
        "bilibili.com".into(),
        "youtube.com".into(),
        "twitter.com".into(),
        "x.com".into(),
        "douyin.com".into(),
        "tiktok.com".into(),
    ]
}
```

`default_v01`：

```rust
        distraction_rules: default_distraction_rules(),
```

`lib.rs` 的 policy 再导出里加上 `default_distraction_rules`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core -- policy`

Expected: PASS。再跑：`cargo test --offline -p gamelife-core`

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/policy.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: ship default distraction hosts for obvious entertainment

Give Bilibili and YouTube a path to auto-distraction so they
do not sit in the gray zone next to Overleaf.
EOF
)"
```

---

### Task 3: `hint_sample` + `is_grounded_core_sample`

**Files:**
- Modify: `crates/gamelife-core/src/hint.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（`pub use hint::{hint_sample, is_grounded_core_sample};`）

**Interfaces:**
- Consumes: `url_host`、`host_is_research`、`strip_url_query_fragment`
- Produces:

```rust
pub fn hint_sample(
    sample: &Sample,
    policy: &Policy,
    quests: &[Quest],
    last_core_interaction_ts: Option<i64>,
) -> Hint

pub fn is_grounded_core_sample(sample: &Sample, quests: &[Quest]) -> bool
```

删除 `AWAY_IDLE_SECS` 与「idle≥10 分钟且非阅读 → Away」。锁屏/暂停、Distraction、Side、阅读桥接顺序不变。

`is_core_candidate`：README/设置只忽略**标题**关键词，仍检查路径、去 query 的 URL 关键词、H2 host。Trusted 检查仍在 `hint_sample` 里、在调用 `is_core_candidate` 之前。

- [ ] **Step 1: 写失败测试**

在 `hint.rs` `tests` 追加（沿用现有 `sample` / `hdp_policy` / `hdp_quest` 辅助函数）：

```rust
    #[test]
    fn idle_title_match_is_core_candidate_not_away_and_not_grounded() {
        let s = sample("Cursor", "train.py — HDP", 700);
        assert_eq!(s.document_path, None);
        assert_eq!(s.url, None);
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
        assert!(!is_grounded_core_sample(&s, &hdp_quest()));
    }

    #[test]
    fn document_path_is_grounded() {
        let mut s = sample("Cursor", "train.py", 10);
        s.document_path = Some("/Users/me/HDP/train.py".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
    }

    #[test]
    fn overleaf_host_is_core_and_grounded_without_keyword() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let mut s = sample("Safari", "Overleaf", 10);
        s.url = Some("https://www.overleaf.com/project/abc123".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::CoreCandidate);
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
        assert!(is_grounded_core_sample(&s, &[]));
    }

    #[test]
    fn overleaf_without_trusted_is_unsure() {
        let p = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let mut s = sample("Safari", "Overleaf", 10);
        s.url = Some("https://www.overleaf.com/project/abc123".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::Unsure);
    }

    #[test]
    fn youtube_beats_quest_keyword_in_title() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: default_distraction_rules(),
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let mut s = sample("Safari", "HDP lecture", 5);
        s.url = Some("https://www.youtube.com/watch?v=1".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::Distraction);
    }

    #[test]
    fn readme_title_still_cores_via_overleaf_url() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let mut s = sample("Safari", "README", 5);
        s.url = Some("https://overleaf.com/project/x".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::CoreCandidate);
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
    }
```

在文件顶部 `use crate::policy::{..., default_distraction_rules}`（若测试模块已 use Policy，补上该函数）。先不改 `hint_sample` 行为，让 idle 测试因仍是 Away 而失败。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- hint::tests::idle_title_match_is_core_candidate_not_away_and_not_grounded`

Expected: FAIL

- [ ] **Step 3: 最小实现**

删除 `const AWAY_IDLE_SECS` 及 `idle_seconds >= AWAY_IDLE_SECS && !is_reading` 分支。

```rust
use crate::url::{host_is_research, url_host};
use crate::url::strip_url_query_fragment;

fn quest_keyword_in(hay: &str, quests: &[Quest]) -> bool {
    let lower = hay.to_ascii_lowercase();
    quests.iter().any(|quest| {
        quest.keywords.iter().any(|keyword| {
            lower.contains(&keyword.to_ascii_lowercase())
        })
    })
}

fn stripped_url(sample: &Sample) -> Option<String> {
    sample.url.as_deref().map(strip_url_query_fragment)
}

pub fn is_grounded_core_sample(sample: &Sample, quests: &[Quest]) -> bool {
    if sample
        .document_path
        .as_deref()
        .is_some_and(|p| quest_keyword_in(p, quests))
    {
        return true;
    }
    let Some(url) = stripped_url(sample) else {
        return false;
    };
    if quest_keyword_in(&url, quests) {
        return true;
    }
    url_host(&url).is_some_and(|h| host_is_research(&h))
}

fn is_core_candidate(sample: &Sample, quests: &[Quest]) -> bool {
    let title_hit = !is_readme_or_settings_title(&sample.window_title)
        && quest_keyword_in(&sample.window_title, quests);
    title_hit || is_grounded_core_sample(sample, quests)
}
```

`hint_sample` 在 Trusted 检查之后：`if is_core_candidate(...) { CoreCandidate } else { Unsure }`。

`lib.rs`：`pub use hint::{hint_sample, is_grounded_core_sample};`

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core -- hint`

Expected: PASS。再跑：`cargo test --offline -p gamelife-core`

若现有测试依赖「idle 700 → Away」，按规格改成「非 Away」；不要为了保绿把空闲 Away 加回来。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/hint.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: treat idle as unsure-or-core instead of away

Match quest keywords on URLs and treat research hosts as
grounded evidence so title-only hits cannot stand alone.
EOF
)"
```

---

### Task 4: `grounded_strong_core` 与自动 Core 门

**Files:**
- Modify: `crates/gamelife-core/src/judge.rs`

**Interfaces:**
- Consumes: `is_grounded_core_sample`
- Produces: `SlotEvidence.grounded_strong_core_seconds: i64`

`analyze_slot_evidence` 在累计 `strong_core` 时，若 `Hint::CoreCandidate && idle < LOW_INPUT_IDLE_SECS && is_grounded_core_sample(sample, quests)`，同时加 `grounded_strong`。

`judge_slot` 自动 Core 分支：

```rust
    } else if !quests_empty
        && ev.grounded_strong_core_seconds >= STRONG_CORE_AUTO_SECS
        && activity.side + activity.distraction <= SIDE_DISTRACTION_MAX_FOR_AUTO_CORE
    {
        dominant = Dominant::CoreResearch;
        credited_raw = ev.grounded_strong_core_seconds + reading_bridge;
```

Away 占优、Side/Distraction 占优、灰区视觉/人工仍用原来的 `strong_core`。`credited_core_spans` 不改。

现有 `grid()` 已带 `document_path: Some("/paper/main.tex")` 且关键词 `main.tex`，故 `strong_core_does_not_need_vision` 应仍为自动 Core。不要改那条夹具的路径，除非它红了。

- [ ] **Step 1: 写失败测试**

在 `judge.rs` `tests` 追加辅助与测试：

```rust
    fn title_only_grid(app: &str, title: &str, start: i64, n: usize, every: i64, idle: i64) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                ts: start + i as i64 * every,
                app: app.into(),
                window_title: title.into(),
                url: None,
                document_path: None,
                bundle_id: None,
                idle_seconds: idle,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect()
    }

    fn browser_pol() -> Policy {
        Policy {
            trusted_apps: vec!["Safari".into(), "Cursor".into()],
            distraction_rules: default_distraction_rules(),
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
        }
    }

    #[test]
    fn title_only_thirteen_minutes_is_pending() {
        let samples = title_only_grid("Cursor", "train.py — HDP", 0, 58, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "HDP".into(),
                keywords: vec!["HDP".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn overleaf_thirteen_minutes_auto_cores_without_keyword() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "HDP".into(),
                keywords: vec!["HDP".into()],
            }],
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert!(!out.pending);
        assert!(out.credited_core_seconds >= 780);
        assert_eq!(out.dominant, Dominant::CoreResearch);
    }

    #[test]
    fn idle_overleaf_does_not_auto_core() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 400,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "HDP".into(),
                keywords: vec!["HDP".into()],
            }],
            policy: &browser_pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending || out.credited_core_seconds < 780);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn two_min_grounded_plus_ten_min_idle_not_auto_core() {
        let mut samples = grid("Cursor", "main.tex", 0, 8, 15, 2);
        samples.extend(grid("Cursor", "main.tex", 120, 40, 15, 400));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                keywords: vec!["main.tex".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert!(out.pending);
    }

    #[test]
    fn youtube_dominant_is_distraction_zero_credit() {
        let samples: Vec<Sample> = (0..52)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "HDP lecture".into(),
                url: Some("https://www.youtube.com/watch?v=1".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "HDP".into(),
                keywords: vec!["HDP".into()],
            }],
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert!(!out.pending);
        assert_eq!(out.dominant, Dominant::Distraction);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn overleaf_without_quests_credits_zero() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[],
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
    }
```

在 `judge.rs` tests 的 use 里加上 `use crate::policy::default_distraction_rules;`。`SlotEvidence` 先不要加字段，让 `title_only_thirteen_minutes_is_pending` 因现网会自动 Core（若标题命中且 idle 低：标题-only 在改门之前可能仍自动 Core）而失败。

注意：改 hint 之后标题-only 已是 `CoreCandidate` 且 idle=2，现网自动 Core 看 `strong_core`，该测试在改门之前应为 FAIL（pending 为假）。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- judge::tests::title_only_thirteen_minutes_is_pending`

Expected: FAIL

- [ ] **Step 3: 最小实现**

`SlotEvidence` 增加 `pub grounded_strong_core_seconds: i64`。

`analyze_slot_evidence`：

```rust
    let mut grounded_strong = 0_i64;
    // inside Observed(CoreCandidate) if idle < LOW_INPUT_IDLE_SECS:
                    Hint::CoreCandidate if idle < LOW_INPUT_IDLE_SECS => {
                        strong_core += secs;
                        if let Some(sample) = span.sample_index.and_then(|i| samples.get(i)) {
                            if is_grounded_core_sample(sample, quests) {
                                grounded_strong += secs;
                            }
                        }
                    }
```

struct 填充 `grounded_strong_core_seconds: grounded_strong`。

`judge_slot` 用 `let grounded_strong_core = ev.grounded_strong_core_seconds;`，自动 Core 条件与 credited 按本 Task Interfaces。

`use crate::hint::is_grounded_core_sample;`（`hint_sample` 已在用）。

若 `vision_ctx` 或其它处用结构更新语法构造 `SlotEvidence`，一并补字段。当前只有 `analyze_slot_evidence` 一处构造。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core -- judge`

Expected: PASS。再跑：`cargo test --offline -p gamelife-core`

Isaac Sim / 窗口族 / 灰区无图测试必须仍绿。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/judge.rs
git commit -m "$(cat <<'EOF'
feat: require grounded evidence for automatic core credit

Title-only quest hits stay in the gray zone; Overleaf hosts
and real document paths can still finalize a 13-minute slot.
EOF
)"
```

---

### Task 5: `metadata_decidable` + policy seed v2

**Files:**
- Modify: `src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: `SlotEvidence.grounded_strong_core_seconds`、`default_distraction_rules`、`default_v01`
- Produces:

```rust
pub fn metadata_decidable(
    activity: &ActivitySeconds,
    strong_core: i64,
    grounded_strong_core: i64,
    reading_bridge: i64,
    actual: i64,
    quests_empty: bool,
) -> bool
```

自动 Core 臂：`!quests_empty && grounded_strong_core >= STRONG_CORE_AUTO_SECS && side+distraction <= 60`。Away / Side 臂仍用 `strong_core`。

`seed_default_policy_if_needed`：

- `seeded >= 2` → return。
- `seeded == 0`：现有「空库或空列表则插入 `default_v01()`」逻辑，但 `app_meta_set(..., "2")`。
- `seeded == 1`：读最新政策 JSON。`distraction_rules` 为空（`None` 或 `[]`）→ `INSERT` 新行（其余字段保持，`distraction_rules = default_distraction_rules()`），然后 version=2。非空 → 只写 version=2。禁止 `UPDATE` 旧政策行。

需要把 `STRONG_CORE_AUTO_SECS` 等常量：scheduler 里若已有同名私有常量，保持；自动 Core 比较用 780，与 core 的 `780` 一致。现网 `metadata_decidable` 已用 `STRONG_CORE_AUTO_SECS`——把该处的 `strong_core` 换成 `grounded_strong_core`。

调用点：`resolve`/`tick` 附近 `metadata_decidable(&evidence.activity, evidence.strong_core_seconds, evidence.reading_bridge_seconds, ...)` 改为传入 `evidence.grounded_strong_core_seconds`。

Grep `metadata_decidable(` 更新每一处，包括测试。

- [ ] **Step 1: 写失败测试**

改现有：

```rust
        assert!(metadata_decidable(&activity, 800, 800, 0, 900, false));
```

```rust
        assert!(!metadata_decidable(&activity, 200, 200, 0, 900, false));
```

新增：

```rust
    #[test]
    fn metadata_not_decidable_for_title_only_strong_core() {
        let activity = ActivitySeconds {
            core: 800,
            side: 0,
            distraction: 30,
            ..Default::default()
        };
        assert!(!metadata_decidable(&activity, 800, 0, 0, 900, false));
    }

    #[test]
    fn seed_empty_db_marks_version_2_and_has_d2() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.distraction_rules.iter().any(|r| r == "youtube.com"));
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
    }

    #[test]
    fn seed_v1_empty_distraction_inserts_d2_row() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let old = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&old).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '1')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.distraction_rules.iter().any(|r| r == "youtube.com"));
        assert_eq!(policy.trusted_apps, vec!["Cursor".to_string()]);
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn seed_v1_custom_distraction_not_replaced() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let old = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec!["reddit.com".into()],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: builtin_never_capture(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&old).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '1')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert_eq!(policy.distraction_rules, vec!["reddit.com".to_string()]);
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn seed_v2_empty_distraction_not_refilled() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let empty = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: builtin_never_capture(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&empty).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '2')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.distraction_rules.is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
```

把旧的 `seed_empty_db_inserts_default_v01_and_marks_version` 改成断言 version `"2"` 且含 `youtube.com`，或删除它以免与 `seed_empty_db_marks_version_2_and_has_d2` 重复。不要留两条互相矛盾的空库测试。

`seed_does_not_restore_cleared_user_lists`：在 v2 之后插入空政策再 seed，count 仍为 2（第一次 seed 可能已把空库变成 v2 默认 + 用户空行）。保持「第二次 seed 不把 Trusted 填回去」：先 seed（v2 默认），再插入空 Policy 行，再 seed，`load_policy` 的 Trusted 仍为空。此测试在 version 已是 2 时应直接 return，行为与规格一致。

- [ ] **Step 2: 跑测试确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- scheduler::tests::metadata_not_decidable_for_title_only_strong_core`

Expected: FAIL（少参数或自动 Core 仍看 strong_core）

- [ ] **Step 3: 实现签名、调用点、seed**

`metadata_decidable` 增加 `grounded_strong_core: i64`，自动 Core 臂用它。

生产调用：

```rust
    let decidable = metadata_decidable(
        &evidence.activity,
        evidence.strong_core_seconds,
        evidence.grounded_strong_core_seconds,
        evidence.reading_bridge_seconds,
        actual,
        quests.is_empty(),
    );
```

`seed_default_policy_if_needed` 按 Interfaces 改。v1 补 D2 时：

```rust
    let mut policy: Policy = serde_json::from_str(&json).map_err(|e| {
        DbOpError::Fatal(format!("policy json: {e}"))
    })?;
    if policy.distraction_rules.is_empty() {
        policy.distraction_rules = default_distraction_rules();
        let new_json = serde_json::to_string(&policy)
            .map_err(|e| DbOpError::Fatal(format!("policy seed json: {e}")))?;
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, ?2)",
            params![new_json, now_secs()],
        )
        .map_err(map_rusqlite)?;
    }
    app_meta_set(conn, "policy_seed_version", "2")?;
```

`use gamelife_core::default_distraction_rules`（若尚未导入）。

空库路径把 `"1"` 改成 `"2"`。

- [ ] **Step 4: 跑测试确认通过**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

再跑：`cargo test --offline -p gamelife-core`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: skip vision only for grounded auto-core and seed D2

Keep title-only slots in the gray zone and backfill empty
distraction lists once without rewriting a custom policy.
EOF
)"
```

---

## Self-review

**Spec coverage**

| 规格 | Task |
| --- | --- |
| §2 空闲不是 Away | 3 |
| §2 标题-only 不自动 Core | 4, 5 |
| §2 路径落地自动 Core | 4（现有 `grid` + 新测试） |
| §2 H2 活跃自动 Core / 发呆不自动 | 1, 3, 4 |
| §2 D2 YouTube 自动 Distraction | 2, 3, 4 |
| §2 无 Quest credited=0 | 4 |
| §3 Hint 不拆、不伪造路径 | 全局 + 3 |
| §4 架构流水线 | 3, 4 |
| §5.1 host / Scholar TLD | 1 |
| §5.2 D2 | 2 |
| §5.3 hint / grounded / README+URL | 3 |
| §5.4 grounded_strong / 双 credited 公式 | 4 |
| §5.5 metadata_decidable + seed v2 | 5 |
| §6 失败对照 | 3, 4, 5 |
| §7 测试 | 各 Task |
| §8 不做项 | 全局约束 |
| §9 D/C 不在本计划 | 无对应 Task |

**Placeholder scan:** 无 TBD。objc/URL crate 不引入。

**Type consistency:** `url_host` / `host_is_research` / `is_grounded_core_sample` / `default_distraction_rules` / `grounded_strong_core_seconds` / `metadata_decidable(..., grounded_strong_core, ...)` 在后续 Task 中的名字与 Task 1–4 一致。
