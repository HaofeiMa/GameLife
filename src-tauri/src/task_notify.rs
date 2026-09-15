use crate::config::load_settings;
use crate::db::{app_db_path, load_tasks, migrate, open};
use crate::macos::{
    cancel_all_task_notifications, replace_task_notifications, PendingTaskNotification,
};

pub fn fires_at(start: i64, offsets: &[i64], now: i64) -> Vec<i64> {
    let mut out: Vec<i64> = offsets
        .iter()
        .map(|off| start - off * 60)
        .filter(|t| *t > now)
        .collect();
    out.sort_unstable();
    out
}

fn remind_body(offset: i64) -> String {
    if offset == 0 {
        "准时".into()
    } else {
        format!("开始前 {offset} 分钟")
    }
}

pub fn identifier_for(task_id: &str, offset: i64) -> String {
    format!("gamelife-task-{task_id}-{offset}")
}

pub fn sync_now() {
    if let Err(e) = sync_now_inner() {
        eprintln!("task_notify: {e}");
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn sync_now_inner() -> Result<(), String> {
    let settings = load_settings();
    if !settings.task_notifications {
        cancel_all_task_notifications();
        return Ok(());
    }
    let path = app_db_path().ok_or_else(|| "home dir".to_string())?;
    let conn = open(&path).map_err(|e| format!("{e:?}"))?;
    migrate(&conn).map_err(|e| format!("{e:?}"))?;
    replace_task_notifications(&pending_from_conn(&conn, now_secs()));
    Ok(())
}

fn pending_from_conn(conn: &rusqlite::Connection, now: i64) -> Vec<PendingTaskNotification> {
    let Ok(tasks) = load_tasks(conn) else {
        return vec![];
    };
    let mut out = Vec::new();
    for task in tasks {
        if task.done {
            continue;
        }
        let Some(start) = task.start else {
            continue;
        };
        for fire_at in fires_at(start, &task.remind_offsets, now) {
            let offset = (start - fire_at) / 60;
            out.push(PendingTaskNotification {
                identifier: identifier_for(&task.id, offset),
                title: task.title.clone(),
                body: remind_body(offset),
                fire_at,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_at_drops_past_and_keeps_future() {
        let start = 10_000;
        assert_eq!(fires_at(start, &[0, 15], 9_500), vec![start]);
        assert_eq!(
            fires_at(start, &[0, 15], 9_000),
            vec![start - 15 * 60, start]
        );
        assert!(fires_at(start, &[0], start + 1).is_empty());
    }
}
