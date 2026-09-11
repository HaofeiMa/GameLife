use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub trusted_apps: Vec<String>,
    pub distraction_rules: Vec<String>,
    pub side_project_rules: Vec<String>,
    pub reading_apps: Vec<String>,
    pub never_capture_apps: Vec<String>,
}

struct KnownApp {
    display: &'static str,
    bundle_ids: &'static [&'static str],
}

fn known_app_identities() -> &'static [KnownApp] {
    &[
        KnownApp {
            display: "Cursor",
            bundle_ids: &["com.todesktop.230313mzl4w4u92"],
        },
        KnownApp {
            display: "Visual Studio Code",
            bundle_ids: &["com.microsoft.VSCode"],
        },
        KnownApp {
            display: "Preview",
            bundle_ids: &["com.apple.Preview"],
        },
        KnownApp {
            display: "Zotero",
            bundle_ids: &["org.zotero.zotero"],
        },
        KnownApp {
            display: "Google Chrome",
            bundle_ids: &["com.google.Chrome"],
        },
        KnownApp {
            display: "Safari",
            bundle_ids: &["com.apple.Safari"],
        },
        KnownApp {
            display: "Microsoft Word",
            bundle_ids: &["com.microsoft.Word"],
        },
        KnownApp {
            display: "Pages",
            bundle_ids: &["com.apple.iWork.Pages"],
        },
        KnownApp {
            display: "PDF Expert",
            bundle_ids: &["com.readdle.PDFExpert-Mac"],
        },
        KnownApp {
            display: "Terminal",
            bundle_ids: &["com.apple.Terminal"],
        },
        KnownApp {
            display: "iTerm2",
            bundle_ids: &["com.googlecode.iterm2"],
        },
        KnownApp {
            display: "Warp",
            bundle_ids: &["dev.warp.Warp-Stable"],
        },
        KnownApp {
            display: "TeXShop",
            bundle_ids: &["edu.ucsd.cs.mmccrack.texshop"],
        },
        KnownApp {
            display: "MATLAB",
            bundle_ids: &["com.mathworks.matlab"],
        },
        KnownApp {
            display: "PyCharm",
            bundle_ids: &["com.jetbrains.pycharm", "com.jetbrains.pycharm.ce"],
        },
        KnownApp {
            display: "JupyterLab",
            bundle_ids: &["org.jupyter.jupyterlab-desktop"],
        },
        KnownApp {
            display: "Jupyter",
            bundle_ids: &[],
        },
        KnownApp {
            display: "1Password",
            bundle_ids: &["com.1password.1password", "com.agilebits.onepassword7"],
        },
        KnownApp {
            display: "Bitwarden",
            bundle_ids: &["com.bitwarden.desktop"],
        },
        KnownApp {
            display: "Keychain Access",
            bundle_ids: &["com.apple.keychainaccess"],
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

pub fn matches_app_identity(app: &str, bundle_id: Option<&str>, names: &[String]) -> bool {
    if let Some(bid) = bundle_id {
        for known in known_app_identities() {
            let bundle_hit = known
                .bundle_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(bid));
            if bundle_hit && matches_app_name(known.display, names) {
                return true;
            }
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
    rules.iter().any(|rule| matches_rule_fields(haystacks, rule))
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
}
