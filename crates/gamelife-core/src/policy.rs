use serde::{Deserialize, Serialize};

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

pub fn nonempty_guides(g: &CategoryGuides) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (key, raw) in [
        ("mainline", g.mainline.as_str()),
        ("side", g.side.as_str()),
        ("admin", g.admin.as_str()),
        ("entertainment", g.entertainment.as_str()),
    ] {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            out.push((key, trimmed.to_string()));
        }
    }
    out
}

struct KnownApp {
    display: &'static str,
    bundle_ids: &'static [&'static str],
    /// Windows `image_display_name` (exe stem). Exact, case-insensitive.
    windows_stems: &'static [&'static str],
    /// X11 `WM_CLASS` class (or instance fallback). Exact, case-insensitive.
    linux_classes: &'static [&'static str],
}

const NONE: &[&str] = &[];

fn known_app_identities() -> &'static [KnownApp] {
    &[
        KnownApp {
            display: "Cursor",
            bundle_ids: &["com.todesktop.230313mzl4w4u92"],
            windows_stems: &["Cursor"],
            linux_classes: &["Cursor"],
        },
        KnownApp {
            display: "Visual Studio Code",
            bundle_ids: &["com.microsoft.VSCode"],
            windows_stems: &["Code"],
            linux_classes: &["Code"],
        },
        KnownApp {
            display: "Preview",
            bundle_ids: &["com.apple.Preview"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "Zotero",
            bundle_ids: &["org.zotero.zotero"],
            windows_stems: &["zotero"],
            linux_classes: &["Zotero"],
        },
        KnownApp {
            display: "Google Chrome",
            bundle_ids: &["com.google.Chrome"],
            windows_stems: &["chrome"],
            linux_classes: &["Google-chrome", "google-chrome"],
        },
        KnownApp {
            display: "Safari",
            bundle_ids: &["com.apple.Safari"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "Microsoft Word",
            bundle_ids: &["com.microsoft.Word"],
            windows_stems: &["WINWORD"],
            linux_classes: &["WINWORD"],
        },
        KnownApp {
            display: "Pages",
            bundle_ids: &["com.apple.iWork.Pages"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "PDF Expert",
            bundle_ids: &["com.readdle.PDFExpert-Mac"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "Terminal",
            bundle_ids: &["com.apple.Terminal"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "iTerm2",
            bundle_ids: &["com.googlecode.iterm2"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "Warp",
            bundle_ids: &["dev.warp.Warp-Stable"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "TeXShop",
            bundle_ids: &["edu.ucsd.cs.mmccrack.texshop"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "MATLAB",
            bundle_ids: &["com.mathworks.matlab"],
            windows_stems: &["matlab"],
            linux_classes: &["MATLAB"],
        },
        KnownApp {
            display: "PyCharm",
            bundle_ids: &["com.jetbrains.pycharm", "com.jetbrains.pycharm.ce"],
            windows_stems: &["pycharm64"],
            linux_classes: &["jetbrains-pycharm", "jetbrains-pycharm-ce"],
        },
        KnownApp {
            display: "JupyterLab",
            bundle_ids: &["org.jupyter.jupyterlab-desktop"],
            windows_stems: &["JupyterLab"],
            linux_classes: &["JupyterLab"],
        },
        KnownApp {
            display: "Jupyter",
            bundle_ids: NONE,
            windows_stems: NONE,
            linux_classes: NONE,
        },
        KnownApp {
            display: "1Password",
            bundle_ids: &["com.1password.1password", "com.agilebits.onepassword7"],
            windows_stems: &["1Password"],
            linux_classes: &["1Password"],
        },
        KnownApp {
            display: "Bitwarden",
            bundle_ids: &["com.bitwarden.desktop"],
            windows_stems: &["Bitwarden"],
            linux_classes: &["Bitwarden"],
        },
        KnownApp {
            display: "Keychain Access",
            bundle_ids: &["com.apple.keychainaccess"],
            windows_stems: NONE,
            linux_classes: NONE,
        },
    ]
}

pub fn default_trusted_apps() -> Vec<String> {
    vec![
        "Cursor".into(),
        "Visual Studio Code".into(),
        "Preview".into(),
        "Zotero".into(),
        "Google Chrome".into(),
        "Safari".into(),
        "Microsoft Word".into(),
        "Pages".into(),
        "PDF Expert".into(),
        "Terminal".into(),
        "iTerm2".into(),
        "Warp".into(),
        "TeXShop".into(),
        "MATLAB".into(),
        "PyCharm".into(),
        "JupyterLab".into(),
        "Jupyter".into(),
    ]
}

pub fn default_reading_apps() -> Vec<String> {
    vec![
        "Preview".into(),
        "Zotero".into(),
        "PDF Expert".into(),
        "Microsoft Word".into(),
        "Pages".into(),
    ]
}

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

pub fn default_v01() -> Policy {
    Policy {
        trusted_apps: default_trusted_apps(),
        distraction_rules: default_distraction_rules(),
        side_project_rules: vec![],
        reading_apps: default_reading_apps(),
        never_capture_apps: builtin_never_capture(),
        admin_apps: vec![],
        category_guides: CategoryGuides::default(),
    }
}

pub fn builtin_side_project_rules() -> Vec<String> {
    vec!["GameLife".into()]
}

pub fn builtin_never_capture() -> Vec<String> {
    vec![
        "1Password".into(),
        "Bitwarden".into(),
        "Keychain Access".into(),
    ]
}

pub fn never_capture_removable(app: &str, builtin: &[String]) -> bool {
    let app_lower = app.to_ascii_lowercase();
    !builtin
        .iter()
        .any(|name| app_lower.contains(&name.to_ascii_lowercase()))
}

fn catalog_alias_hit(app: &str, known: &KnownApp) -> bool {
    known
        .windows_stems
        .iter()
        .chain(known.linux_classes)
        .any(|alias| alias.eq_ignore_ascii_case(app))
}

pub fn matches_app_identity(app: &str, bundle_id: Option<&str>, names: &[String]) -> bool {
    for known in known_app_identities() {
        if let Some(bid) = bundle_id {
            let bundle_hit = known
                .bundle_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(bid));
            if bundle_hit && matches_app_name(known.display, names) {
                return true;
            }
        }
        if catalog_alias_hit(app, known) && matches_app_name(known.display, names) {
            return true;
        }
    }
    matches_app_name(app, names)
}

pub(crate) fn matches_rule_fields(haystacks: &[&str], rule: &str) -> bool {
    let needle = rule.to_ascii_lowercase();
    haystacks
        .iter()
        .any(|hay| hay.to_ascii_lowercase().contains(&needle))
}

pub(crate) fn matches_any_rule(haystacks: &[&str], rules: &[String]) -> bool {
    rules
        .iter()
        .any(|rule| matches_rule_fields(haystacks, rule))
}

pub(crate) fn matches_app_name(app: &str, names: &[String]) -> bool {
    let app_lower = app.to_ascii_lowercase();
    names
        .iter()
        .any(|name| app_lower.contains(&name.to_ascii_lowercase()))
}

pub(crate) fn all_side_project_rules(policy: &Policy) -> Vec<String> {
    let mut rules = builtin_side_project_rules();
    rules.extend(policy.side_project_rules.clone());
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_v01_includes_cursor_and_preview() {
        let p = default_v01();
        assert!(p.trusted_apps.iter().any(|a| a == "Cursor"));
        assert!(p.trusted_apps.iter().any(|a| a == "Preview"));
        assert!(p.reading_apps.iter().any(|a| a == "Preview"));
        assert!(p.reading_apps.iter().any(|a| a == "Zotero"));
    }

    #[test]
    fn default_v01_roundtrips_json() {
        let p = default_v01();
        let json = serde_json::to_string(&p).unwrap();
        let back: Policy = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn cursor_matches_via_bundle_id_even_if_display_name_localized() {
        let names = vec!["Cursor".into()];
        assert!(matches_app_identity(
            "光标",
            Some("com.todesktop.230313mzl4w4u92"),
            &names,
        ));
    }

    #[test]
    fn isaac_matches_display_name_without_catalog() {
        let names = vec!["Isaac Sim".into()];
        assert!(matches_app_identity("Isaac Sim", None, &names));
    }

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
        assert!(matches_app_identity(
            "jetbrains-pycharm",
            None,
            &p.trusted_apps
        ));
        assert!(matches_app_identity("JupyterLab", None, &p.trusted_apps));
        let never = builtin_never_capture();
        assert!(!never_capture_removable("1Password", &never));
        assert!(!never_capture_removable("Bitwarden", &never));
        assert!(matches_app_identity("1Password", None, &never));
        assert!(matches_app_identity("Bitwarden", None, &never));
    }
}
