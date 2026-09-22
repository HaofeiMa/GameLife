use std::collections::BTreeMap;

use crate::task::{
    ListRole, PRESET_CHORE_ID, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID, PRESET_SIDE_ID,
};

pub struct RemoteProject {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
}

pub fn parse_role(raw: &str) -> Option<ListRole> {
    if raw.is_empty() || raw == "ignore" {
        return None;
    }
    match raw {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
}

pub fn list_id_for_role(role: ListRole) -> Option<&'static str> {
    match role {
        ListRole::Mainline => Some(PRESET_MAINLINE_ID),
        ListRole::Side => Some(PRESET_SIDE_ID),
        ListRole::Longterm => Some(PRESET_LONGTERM_ID),
        ListRole::Chore => Some(PRESET_CHORE_ID),
        ListRole::Custom => None,
    }
}

pub fn write_target<'a>(
    projects: &'a [RemoteProject],
    roles: &BTreeMap<String, String>,
    role: ListRole,
) -> Option<&'a RemoteProject> {
    projects
        .iter()
        .filter(|project| {
            roles
                .get(&project.id)
                .and_then(|raw| parse_role(raw))
                .is_some_and(|parsed| parsed == role)
        })
        .min_by(|left, right| (left.sort_order, &left.id).cmp(&(right.sort_order, &right.id)))
}

pub fn stamp_missing_roles(projects: &[RemoteProject], roles: &mut BTreeMap<String, String>) {
    for project in projects {
        roles
            .entry(project.id.clone())
            .or_insert_with(|| "ignore".into());
    }
}

pub fn map_times(
    all_day: bool,
    start: Option<i64>,
    end: Option<i64>,
    day_start: i64,
    next_day_start: i64,
) -> (Option<i64>, Option<i64>, bool) {
    if all_day {
        (Some(day_start), Some(next_day_start), true)
    } else {
        (start, end, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn proj(id: &str, sort_order: i64) -> RemoteProject {
        RemoteProject {
            id: id.into(),
            name: id.into(),
            sort_order,
        }
    }

    #[test]
    fn write_target_picks_smallest_sort_order_then_id() {
        let projects = vec![proj("b", -3), proj("a", -3), proj("c", -10)];
        let mut roles = BTreeMap::new();
        roles.insert("a".into(), "mainline".into());
        roles.insert("b".into(), "mainline".into());
        roles.insert("c".into(), "mainline".into());
        roles.insert("z".into(), "ignore".into());
        assert_eq!(
            write_target(&projects, &roles, ListRole::Mainline)
                .unwrap()
                .id,
            "c"
        );
        let only = vec![proj("b", -3), proj("a", -3)];
        assert_eq!(
            write_target(&only, &roles, ListRole::Mainline)
                .unwrap()
                .id,
            "a"
        );
        assert!(write_target(&projects, &roles, ListRole::Side).is_none());
    }

    #[test]
    fn stamp_missing_roles_defaults_ignore_and_keeps_existing() {
        let projects = vec![proj("new", 1), proj("old", 2)];
        let mut roles = BTreeMap::new();
        roles.insert("old".into(), "side".into());
        stamp_missing_roles(&projects, &mut roles);
        assert_eq!(roles.get("new").map(String::as_str), Some("ignore"));
        assert_eq!(roles.get("old").map(String::as_str), Some("side"));
    }

    #[test]
    fn map_times_all_day_uses_caller_bounds() {
        assert_eq!(
            map_times(true, Some(1), Some(2), 100, 200),
            (Some(100), Some(200), true)
        );
        assert_eq!(
            map_times(false, None, Some(5), 100, 200),
            (None, Some(5), false)
        );
    }

    #[test]
    fn list_ids_match_presets() {
        assert_eq!(
            list_id_for_role(ListRole::Mainline),
            Some(PRESET_MAINLINE_ID)
        );
        assert_eq!(list_id_for_role(ListRole::Chore), Some(PRESET_CHORE_ID));
        assert_eq!(list_id_for_role(ListRole::Custom), None);
        assert_eq!(parse_role("ignore"), None);
        assert_eq!(parse_role("longterm"), Some(ListRole::Longterm));
    }
}
