#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub trusted_apps: Vec<String>,
    pub distraction_rules: Vec<String>,
    pub side_project_rules: Vec<String>,
    pub reading_apps: Vec<String>,
    pub never_capture_apps: Vec<String>,
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
