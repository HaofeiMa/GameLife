# Cross-platform identity aliases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Windows stems (`chrome`) and Linux `WM_CLASS` (`Google-chrome`, `Code`) hit the same Policy lists as macOS display names, without putting those short names into the lists.

**Architecture:** Extend `known_app_identities` with exact, case-insensitive `windows_stems` and `linux_classes`. `matches_app_identity` resolves an observed `app` through that catalog onto `known.display`, then checks the list. Policy JSON is unchanged.

**Tech Stack:** `gamelife-core` only (`chrono` / `serde` / `serde_json`). In-process tests.

**Spec:** `docs/superpowers/specs/2026-09-14-cross-platform-observation-design.md` §6.3–6.4, §11.

## Global Constraints

- Do not add `chrome` / `Code` as substrings to `trusted_apps` or any default Policy list.
- Alias match is exact (`eq_ignore_ascii_case`), not `contains`.
- Do not fabricate Windows bundle ids. Linux `_GTK_APPLICATION_ID` already uses the existing `bundle_ids` branch.
- Tests must not set `HOME` / `TZ`.
- Gate: `cargo test --offline -p gamelife-core`.
- Do not commit the leftover UI / hint working tree.

---

### Task 1: Catalog aliases for `matches_app_identity`

**Files:**
- Modify: `crates/gamelife-core/src/policy.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: existing `matches_app_identity(app, bundle_id, names)`
- Produces: same signature; catalog apps in spec §6.3 resolve from Windows stem / Linux class onto `display`

- [x] **Step 1: Write the failing tests** (production change that would fail them: dropping the `chrome` → `Google Chrome` alias)

```rust
#[test]
fn windows_chrome_stem_matches_google_chrome_on_the_list() {
    let names = vec!["Google Chrome".into()];
    assert!(matches_app_identity("chrome", None, &names));
    assert!(matches_app_identity("CHROME", None, &names));
}

#[test]
fn linux_chrome_class_matches_google_chrome_on_the_list() {
    let names = vec!["Google Chrome".into()];
    assert!(matches_app_identity("Google-chrome", None, &names));
    assert!(matches_app_identity("google-chrome", None, &names));
}

#[test]
fn linux_code_class_matches_visual_studio_code_on_the_list() {
    let names = vec!["Visual Studio Code".into()];
    assert!(matches_app_identity("Code", None, &names));
}

#[test]
fn alias_does_not_use_contains() {
    let names = vec!["Google Chrome".into()];
    assert!(!matches_app_identity("chromedriver", None, &names));
}

#[test]
fn default_trusted_list_does_not_contain_short_stems() {
    let p = default_v01();
    assert!(!p.trusted_apps.iter().any(|a| a == "chrome" || a == "Code"));
    assert!(matches_app_identity("chrome", None, &p.trusted_apps));
    assert!(matches_app_identity("Code", None, &p.trusted_apps));
}

#[test]
fn remaining_spec_aliases_resolve() {
    let p = default_v01();
    assert!(matches_app_identity("WINWORD", None, &p.trusted_apps));
    assert!(matches_app_identity("zotero", None, &p.trusted_apps));
    assert!(matches_app_identity("matlab", None, &p.trusted_apps));
    assert!(matches_app_identity("pycharm64", None, &p.trusted_apps));
    assert!(matches_app_identity("jetbrains-pycharm", None, &p.trusted_apps));
    assert!(matches_app_identity("JupyterLab", None, &p.trusted_apps));
    let never = builtin_never_capture();
    assert!(!never_capture_removable("1Password", &never));
    assert!(!never_capture_removable("Bitwarden", &never));
    assert!(matches_app_identity("1Password", None, &never));
    assert!(matches_app_identity("Bitwarden", None, &never));
}
```

- [x] **Step 2: Run tests — expect fail** because `chrome` does not contain `google chrome`

Run: `cargo test --offline -p gamelife-core policy::tests::windows_chrome_stem_matches_google_chrome_on_the_list`

- [x] **Step 3: Minimal implementation** — `windows_stems` / `linux_classes` on `KnownApp`; resolve in `matches_app_identity` before `matches_app_name`. Spec §6.3 table only.

- [x] **Step 4: Run** `cargo test --offline -p gamelife-core`

- [x] **Step 5: Commit** `feat: map Windows and X11 app names onto the policy catalog`

---

### Task 2: Mark the spec landed

**Files:**
- Modify: `docs/superpowers/specs/2026-09-14-cross-platform-observation-design.md` §12

- [x] **Step 1:** §12: aliases are in `known_app_identities`; backends still untested on real machines.
- [x] **Step 2: Commit** `docs: note that identity aliases landed`
