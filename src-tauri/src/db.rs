use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::db_error::{map_rusqlite, DbOpError};
use gamelife_core::shop::{
    can_start_entertainment, entertainment_remaining_secs, has_entertainment_timer,
    validate_redeem, validate_wish, RedeemError, Wish, WishError, WishKind,
};
use gamelife_core::{preset_lists, ListRole, Task, TaskList, TaskRange};

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS heartbeat (id INTEGER PRIMARY KEY CHECK (id=1), ts INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS quest_versions (id INTEGER PRIMARY KEY, day TEXT NOT NULL, json TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS policy_versions (id INTEGER PRIMARY KEY, json TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS days (
  day TEXT PRIMARY KEY,
  settled_at INTEGER,
  outcome TEXT
);
CREATE TABLE IF NOT EXISTS samples (
  ts INTEGER PRIMARY KEY,
  day TEXT NOT NULL,
  app TEXT, title TEXT, url TEXT, path TEXT,
  document_path TEXT,
  bundle_id TEXT,
  idle_seconds INTEGER, locked INTEGER, paused INTEGER,
  secure_input INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS slots (
  day TEXT NOT NULL,
  slot_start INTEGER NOT NULL,
  quest_version_id INTEGER,
  policy_version_id INTEGER,
  capture_scheduled_at INTEGER,
  capture_status TEXT,
  screenshot_path TEXT,
  captured_at INTEGER,
  capture_context_json TEXT,
  category TEXT,
  status TEXT,
  activity_json TEXT,
  credited_core_seconds INTEGER,
  credited_side_seconds INTEGER,
  credited_chore_seconds INTEGER,
  observed_seconds INTEGER,
  used_vision INTEGER,
  task_snapshot_json TEXT,
  PRIMARY KEY (day, slot_start)
);
CREATE TABLE IF NOT EXISTS ledger (
  reward_event_key TEXT PRIMARY KEY,
  day TEXT NOT NULL,
  ts INTEGER NOT NULL,
  coin_delta INTEGER NOT NULL,
  xp_delta INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS wishes (
  id TEXT PRIMARY KEY, name TEXT, kind TEXT, price INTEGER, duration_minutes INTEGER, notes TEXT,
  archived INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS redemptions (
  redemption_id TEXT PRIMARY KEY,
  wish_id TEXT NOT NULL,
  ts INTEGER NOT NULL,
  name TEXT,
  duration_minutes INTEGER
);
CREATE TABLE IF NOT EXISTS entertainment_sessions (
  redemption_id TEXT PRIMARY KEY,
  wish_id TEXT NOT NULL,
  name TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  ends_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS freeze_uses (
  protected_date TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS misclassification_reports (
  id INTEGER PRIMARY KEY,
  day TEXT NOT NULL,
  slot_start INTEGER NOT NULL,
  note TEXT NOT NULL,
  ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS app_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS task_lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, sort INTEGER NOT NULL, role TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL,
  title TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,
  start INTEGER,
  end INTEGER,
  range TEXT
);
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
";

const TARGET_USER_VERSION: i32 = 3;

const WAVE1_COLUMNS: &[(&str, &str, &str)] = &[
    ("samples", "document_path", "TEXT"),
    ("samples", "bundle_id", "TEXT"),
    ("samples", "secure_input", "INTEGER NOT NULL DEFAULT 0"),
    ("slots", "screenshot_path", "TEXT"),
    ("slots", "captured_at", "INTEGER"),
    ("slots", "capture_context_json", "TEXT"),
];

/// Tables that phase two merges across devices. Each carries `device_id` so a
/// row can be attributed to the machine that observed it.
///
/// The local primary keys deliberately do **not** gain a device dimension: one
/// device has one database, so `(day, slot_start)` stays unique here. The
/// device dimension only matters inside `merged.db`, which is why adding these
/// columns touches no query in `resolve.rs` / `scheduler.rs` / `commands.rs`.
const DEVICE_TAGGED_TABLES: &[&str] = &[
    "slots",
    "ledger",
    "samples",
    "app_day_stats",
    "host_day_stats",
    "days",
    "policy_versions",
    "misclassification_reports",
];

pub const DEVICE_ID_KEY: &str = "device_id";

pub fn open(path: &Path) -> Result<Connection, DbOpError> {
    let conn = Connection::open(path).map_err(map_rusqlite)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(map_rusqlite)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<(), DbOpError> {
    conn.execute_batch(SCHEMA).map_err(map_rusqlite)?;
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(map_rusqlite)?;
    if version < 2 {
        for (table, column, decl) in WAVE1_COLUMNS {
            add_column_if_missing(conn, table, column, decl)?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
        )
        .map_err(map_rusqlite)?;
    }
    if version < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ticktick_cache (
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
             );",
        )
        .map_err(map_rusqlite)?;
    }
    if version < TARGET_USER_VERSION {
        conn.pragma_update(None, "user_version", TARGET_USER_VERSION)
            .map_err(map_rusqlite)?;
    }
    add_column_if_missing(conn, "wishes", "archived", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "redemptions", "name", "TEXT")?;
    add_column_if_missing(conn, "redemptions", "duration_minutes", "INTEGER")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entertainment_sessions (
           redemption_id TEXT PRIMARY KEY,
           wish_id TEXT NOT NULL,
           name TEXT NOT NULL,
           started_at INTEGER NOT NULL,
           ends_at INTEGER NOT NULL
         );",
    )
    .map_err(map_rusqlite)?;
    add_column_if_missing(conn, "slots", "task_snapshot_json", "TEXT")?;
    add_column_if_missing(conn, "slots", "credited_side_seconds", "INTEGER")?;
    add_column_if_missing(conn, "slots", "credited_chore_seconds", "INTEGER")?;
    let device_id = local_device_id(conn)?;
    tag_device_rows(conn, &device_id)?;
    seed_preset_lists_if_empty(conn)?;
    seed_example_wishes_if_empty(conn)?;
    Ok(())
}

pub fn list_role_sql(role: ListRole) -> &'static str {
    match role {
        ListRole::Mainline => "mainline",
        ListRole::Side => "side",
        ListRole::Longterm => "longterm",
        ListRole::Chore => "chore",
        ListRole::Custom => "custom",
    }
}

fn seed_preset_lists_if_empty(conn: &Connection) -> Result<(), DbOpError> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM task_lists", [], |r| r.get(0))
        .map_err(map_rusqlite)?;
    if n > 0 {
        return Ok(());
    }
    for list in preset_lists() {
        conn.execute(
            "INSERT INTO task_lists (id, name, sort, role) VALUES (?1, ?2, ?3, ?4)",
            params![list.id, list.name, list.sort, list_role_sql(list.role)],
        )
        .map_err(map_rusqlite)?;
    }
    Ok(())
}

fn seed_example_wishes_if_empty(conn: &Connection) -> Result<(), DbOpError> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM wishes", [], |r| r.get(0))
        .map_err(map_rusqlite)?;
    if n > 0 {
        return Ok(());
    }
    let seeds: [(&str, &str, &str, i64, Option<i64>); 6] = [
        ("seed-coin-tea", "一杯奶茶", "coin", 32, None),
        ("seed-coin-takeout", "一顿外卖", "coin", 64, None),
        ("seed-coin-book", "一本新书", "coin", 48, None),
        ("seed-xp-bilibili", "B 站 45 分钟", "xp", 20, Some(45)),
        ("seed-xp-game", "游戏 30 分钟", "xp", 18, Some(30)),
        ("seed-xp-shorts", "短视频 15 分钟", "xp", 10, Some(15)),
    ];
    for (id, name, kind, price, duration) in seeds {
        conn.execute(
            "INSERT INTO wishes (id, name, kind, price, duration_minutes, archived) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id, name, kind, price, duration],
        )
        .map_err(map_rusqlite)?;
    }
    Ok(())
}

pub fn parse_list_role(role: &str) -> ListRole {
    match role {
        "mainline" => ListRole::Mainline,
        "side" => ListRole::Side,
        "longterm" => ListRole::Longterm,
        "chore" => ListRole::Chore,
        _ => ListRole::Custom,
    }
}

pub fn load_task_lists(conn: &Connection) -> Result<Vec<TaskList>, DbOpError> {
    let mut stmt = conn
        .prepare("SELECT id, name, sort, role FROM task_lists ORDER BY sort, id")
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TaskList {
                id: r.get(0)?,
                name: r.get(1)?,
                sort: r.get(2)?,
                role: parse_list_role(&r.get::<_, String>(3)?),
            })
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

pub fn load_tasks(conn: &Connection) -> Result<Vec<Task>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, list_id, title, done, start, end, range FROM tasks ORDER BY start, id",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            let range: Option<String> = r.get(6)?;
            Ok(Task {
                id: r.get(0)?,
                list_id: r.get(1)?,
                title: r.get(2)?,
                done: r.get::<_, i64>(3)? != 0,
                start: r.get(4)?,
                end: r.get(5)?,
                range: match range.as_deref() {
                    Some("week") => Some(TaskRange::Week),
                    Some("month") => Some(TaskRange::Month),
                    _ => None,
                },
            })
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, DbOpError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(map_rusqlite)?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(map_rusqlite)?;
    for name in names {
        if name.map_err(map_rusqlite)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<(), DbOpError> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
    let mut last_err = None;
    for _ in 0..5 {
        match conn.execute(&sql, []) {
            Ok(_) => return Ok(()),
            Err(err) => {
                let msg = err.to_string().to_ascii_lowercase();
                if msg.contains("duplicate column") {
                    return Ok(());
                }
                match map_rusqlite(err) {
                    DbOpError::Busy => {
                        last_err = Some(DbOpError::Busy);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    other => return Err(other),
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| DbOpError::Busy))
}

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, DbOpError> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .map_err(map_rusqlite)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), DbOpError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

/// This installation's stable identity, minted on first use and kept in
/// `app_meta`. It names the remote directory a device uploads into, and in
/// phase two it is what makes slot ownership decidable.
///
/// The value is never regenerated once written — changing it would orphan the
/// device's remote directory and, worse, re-attribute its history.
pub fn local_device_id(conn: &Connection) -> Result<String, DbOpError> {
    if let Some(existing) = meta_get(conn, DEVICE_ID_KEY)? {
        let existing = existing.trim().to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| DbOpError::Fatal(format!("random: {e}")))?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    meta_set(conn, DEVICE_ID_KEY, &id)?;
    Ok(id)
}

/// Phase-two tagging: give every merged table a `device_id`, stamp the rows
/// that predate the column, and keep new rows stamped.
///
/// The trigger is what keeps new rows tagged. There are a dozen production
/// insert sites across `sampler.rs` / `scheduler.rs` / `resolve.rs` /
/// `commands.rs`; editing all of them means a future one will be missed, and a
/// silently untagged row is exactly the kind of quiet corruption this project
/// goes out of its way to avoid. The trigger reads the id from `app_meta`
/// rather than baking it in, so a restored snapshot re-stamps itself with the
/// id that snapshot carries.
///
/// Adding a column is idempotent and carries no version semantics, so
/// `user_version` stays 3.
fn tag_device_rows(conn: &Connection, device_id: &str) -> Result<(), DbOpError> {
    for table in DEVICE_TAGGED_TABLES {
        add_column_if_missing(conn, table, "device_id", "TEXT NOT NULL DEFAULT ''")?;
        conn.execute(
            &format!("UPDATE {table} SET device_id = ?1 WHERE device_id = ''"),
            params![device_id],
        )
        .map_err(map_rusqlite)?;
        conn.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS tag_{table}_device;
             CREATE TRIGGER tag_{table}_device AFTER INSERT ON {table}
             WHEN NEW.device_id = ''
             BEGIN
               UPDATE {table}
                  SET device_id = COALESCE(
                        (SELECT value FROM app_meta WHERE key = '{DEVICE_ID_KEY}'), '')
                WHERE rowid = NEW.rowid;
             END;"
        ))
        .map_err(map_rusqlite)?;
    }
    Ok(())
}

pub fn app_db_path() -> Option<std::path::PathBuf> {
    crate::platform::app_support_dir().map(|dir| dir.join("gamelife.db"))
}

pub fn write_heartbeat(conn: &Connection) -> Result<(), DbOpError> {
    migrate(conn)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO heartbeat (id, ts) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET ts = excluded.ts",
        params![ts],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

pub fn write_heartbeat_at_default_path() -> Result<(), DbOpError> {
    let path = app_db_path().ok_or_else(|| DbOpError::Fatal("home dir".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DbOpError::Fatal(format!("create db dir: {e}")))?;
    }
    let conn = open(&path)?;
    write_heartbeat(&conn)
}

pub fn insert_ledger(
    conn: &Connection,
    key: &str,
    day: &str,
    coin: i64,
    xp: i64,
) -> Result<(), DbOpError> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, day, ts, coin, xp],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn map_wish_error(e: WishError) -> DbOpError {
    match e {
        WishError::EmptyName => DbOpError::Rejected("empty_name".into()),
        WishError::NameTooLong => DbOpError::Rejected("name_too_long".into()),
        WishError::NonPositivePrice => DbOpError::Rejected("non_positive_price".into()),
        WishError::EntertainmentNeedsDuration => {
            DbOpError::Rejected("entertainment_needs_duration".into())
        }
        WishError::CoinMustNotHaveDuration => {
            DbOpError::Rejected("coin_must_not_have_duration".into())
        }
    }
}

fn parse_wish_kind(kind: &str, duration: Option<i64>) -> Result<WishKind, DbOpError> {
    match kind {
        "coin" => {
            if duration.is_some() {
                return Err(DbOpError::Rejected("coin_must_not_have_duration".into()));
            }
            Ok(WishKind::Coin)
        }
        "xp" => Ok(WishKind::Xp {
            duration_minutes: duration,
        }),
        _ => Err(DbOpError::Rejected("invalid_kind".into())),
    }
}

pub fn insert_wish(
    conn: &Connection,
    wish_id: &str,
    name: &str,
    kind: &str,
    price: i64,
    duration: Option<i64>,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let wish_kind = parse_wish_kind(kind, duration)?;
    validate_wish(name, &wish_kind, price).map_err(map_wish_error)?;
    let kind_str = match &wish_kind {
        WishKind::Coin => "coin",
        WishKind::Xp { .. } => "xp",
    };
    let duration_minutes = match &wish_kind {
        WishKind::Coin => None,
        WishKind::Xp { duration_minutes } => *duration_minutes,
    };
    conn.execute(
        "INSERT INTO wishes (id, name, kind, price, duration_minutes, archived) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![wish_id, name.trim(), kind_str, price, duration_minutes],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

pub fn update_wish(
    conn: &Connection,
    wish_id: &str,
    name: &str,
    price: i64,
    duration: Option<i64>,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT kind, COALESCE(archived, 0) FROM wishes WHERE id = ?1",
            [wish_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let (kind_str, archived) =
        row.ok_or_else(|| DbOpError::Rejected("wish_missing".into()))?;
    if archived != 0 {
        return Err(DbOpError::Rejected("wish_archived".into()));
    }
    let wish_kind = if kind_str == "coin" {
        if duration.is_some() {
            return Err(DbOpError::Rejected("coin_must_not_have_duration".into()));
        }
        WishKind::Coin
    } else {
        WishKind::Xp {
            duration_minutes: duration,
        }
    };
    validate_wish(name, &wish_kind, price).map_err(map_wish_error)?;
    let duration_minutes = match &wish_kind {
        WishKind::Coin => None,
        WishKind::Xp { duration_minutes } => *duration_minutes,
    };
    let n = conn
        .execute(
            "UPDATE wishes SET name = ?1, price = ?2, duration_minutes = ?3
             WHERE id = ?4 AND COALESCE(archived, 0) = 0",
            params![name.trim(), price, duration_minutes, wish_id],
        )
        .map_err(map_rusqlite)?;
    if n == 0 {
        return Err(DbOpError::Rejected("wish_missing".into()));
    }
    Ok(())
}

pub fn archive_wish(conn: &Connection, wish_id: &str) -> Result<(), DbOpError> {
    migrate(conn)?;
    let n = conn
        .execute(
            "UPDATE wishes SET archived = 1 WHERE id = ?1",
            [wish_id],
        )
        .map_err(map_rusqlite)?;
    if n == 0 {
        return Err(DbOpError::Rejected("wish_missing".into()));
    }
    Ok(())
}

pub fn load_active_session(
    conn: &Connection,
    now: i64,
) -> Result<Option<(String, i64, i64)>, DbOpError> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT name, ends_at FROM entertainment_sessions
             WHERE ends_at > ?1
             ORDER BY ends_at DESC LIMIT 1",
            [now],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(map_rusqlite)?;
    Ok(row.map(|(name, ends_at)| {
        (name, ends_at, entertainment_remaining_secs(now, ends_at))
    }))
}

fn map_redeem_error(e: RedeemError) -> DbOpError {
    match e {
        RedeemError::ShopLocked => DbOpError::Rejected("shop_locked".into()),
        RedeemError::Insufficient => DbOpError::Rejected("insufficient".into()),
        RedeemError::EntertainmentNeedsDuration => {
            DbOpError::Rejected("entertainment_needs_duration".into())
        }
        RedeemError::EntertainmentInProgress => {
            DbOpError::Rejected("entertainment_in_progress".into())
        }
        RedeemError::WishArchived => DbOpError::Rejected("wish_archived".into()),
        RedeemError::WishMissing => DbOpError::Rejected("wish_missing".into()),
        RedeemError::FinalSlotImmutable => DbOpError::Fatal("final".into()),
    }
}

fn begin_write_tx(conn: &mut Connection) -> Result<Transaction<'_>, DbOpError> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_rusqlite)
}

fn reject_if_multiple_active_sessions(
    tx: &Transaction<'_>,
    now: i64,
) -> Result<(), DbOpError> {
    let n: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM entertainment_sessions WHERE ends_at > ?1",
            [now],
            |r| r.get(0),
        )
        .map_err(map_rusqlite)?;
    if n > 1 {
        Err(DbOpError::Rejected("entertainment_in_progress".into()))
    } else {
        Ok(())
    }
}

fn insert_entertainment_session(
    tx: &Transaction<'_>,
    redemption_id: &str,
    wish_id: &str,
    name: &str,
    now: i64,
    duration_minutes: i64,
) -> Result<(), DbOpError> {
    let ends_at = now + duration_minutes * 60;
    tx.execute(
        "INSERT INTO entertainment_sessions (redemption_id, wish_id, name, started_at, ends_at) VALUES (?1,?2,?3,?4,?5)",
        params![redemption_id, wish_id, name, now, ends_at],
    )
    .map_err(map_rusqlite)?;
    reject_if_multiple_active_sessions(tx, now)
}

pub fn redeem(
    conn: &mut Connection,
    credited_today: i64,
    day: &str,
    wish: &Wish,
    wish_name: &str,
    now: i64,
    redemption_id: &str,
) -> Result<(), DbOpError> {
    let tx = begin_write_tx(conn)?;
    let coin: i64 = tx
        .query_row("SELECT COALESCE(SUM(coin_delta),0) FROM ledger", [], |r| r.get(0))
        .map_err(map_rusqlite)?;
    let xp: i64 = tx
        .query_row(
            "SELECT COALESCE(SUM(xp_delta),0) FROM ledger WHERE day=?1",
            [day],
            |r| r.get(0),
        )
        .map_err(map_rusqlite)?;
    let redemption_exists: bool = tx
        .query_row(
            "SELECT 1 FROM redemptions WHERE redemption_id = ?1",
            [redemption_id],
            |_| Ok(true),
        )
        .optional()
        .map_err(map_rusqlite)?
        .is_some();
    if redemption_exists {
        return Err(DbOpError::AlreadyApplied);
    }
    validate_redeem(credited_today, coin, xp, wish).map_err(map_redeem_error)?;
    let active_ends_at: Option<i64> = tx
        .query_row(
            "SELECT MAX(ends_at) FROM entertainment_sessions",
            [],
            |r| r.get(0),
        )
        .map_err(map_rusqlite)?;
    if has_entertainment_timer(&wish.kind) && !can_start_entertainment(now, active_ends_at) {
        return Err(DbOpError::Rejected("entertainment_in_progress".into()));
    }
    let duration_minutes = match &wish.kind {
        WishKind::Xp { duration_minutes } => duration_minutes,
        WishKind::Coin => &None,
    };
    tx.execute(
        "INSERT INTO redemptions (redemption_id, wish_id, ts, name, duration_minutes) VALUES (?1,?2,?3,?4,?5)",
        params![redemption_id, wish.id, now, wish_name, duration_minutes],
    )
    .map_err(map_rusqlite)?;
    let key = format!("shop_spend:{redemption_id}");
    let (c, x) = match wish.kind {
        WishKind::Coin => (-wish.price, 0),
        WishKind::Xp { .. } => (0, -wish.price),
    };
    tx.execute(
        "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, day, now, c, x],
    )
    .map_err(map_rusqlite)?;
    if has_entertainment_timer(&wish.kind) {
        let duration = match &wish.kind {
            WishKind::Xp {
                duration_minutes: Some(d),
            } => *d,
            _ => unreachable!(),
        };
        insert_entertainment_session(&tx, redemption_id, &wish.id, wish_name, now, duration)?;
    }
    tx.commit().map_err(map_rusqlite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn second_insert_same_key_is_already_applied() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_ledger(&conn, "validated_coin:2026-09-10:1", "2026-09-10", 1, 0).unwrap();
        let e = insert_ledger(&conn, "validated_coin:2026-09-10:1", "2026-09-10", 1, 0)
            .unwrap_err();
        assert!(matches!(e, DbOpError::AlreadyApplied));
    }

    #[test]
    fn notnull_violation_is_fatal_not_already_applied() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let e = conn
            .execute(
                "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta) VALUES (?1, NULL, 1, 1, 0)",
                params!["k"],
            )
            .unwrap_err();
        let mapped = map_rusqlite(e);
        assert!(matches!(mapped, DbOpError::Fatal(_)));
    }

    #[test]
    fn open_temp_file_is_usable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gamelife.db");
        let conn = open(&path).unwrap();
        migrate(&conn).unwrap();
        insert_ledger(&conn, "validated_coin:2026-09-10:1", "2026-09-10", 1, 0).unwrap();
    }

    #[test]
    fn write_heartbeat_upserts_row_id_one() {
        let conn = Connection::open_in_memory().unwrap();
        write_heartbeat(&conn).unwrap();
        let ts: i64 = conn
            .query_row("SELECT ts FROM heartbeat WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(ts > 0);

        write_heartbeat(&conn).unwrap();
        let ts2: i64 = conn
            .query_row("SELECT ts FROM heartbeat WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(ts2 >= ts);
    }

    fn xp_sum(conn: &Connection, day: &str) -> i64 {
        conn.query_row(
            "SELECT COALESCE(SUM(xp_delta),0) FROM ledger WHERE day=?1",
            [day],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn xp_wish(price: i64) -> gamelife_core::shop::Wish {
        gamelife_core::shop::Wish {
            id: "coffee".into(),
            kind: gamelife_core::shop::WishKind::Xp {
                duration_minutes: None,
            },
            price,
        }
    }

    fn timed_xp(price: i64) -> gamelife_core::shop::Wish {
        gamelife_core::shop::Wish {
            id: "video".into(),
            kind: gamelife_core::shop::WishKind::Xp {
                duration_minutes: Some(30),
            },
            price,
        }
    }

    #[test]
    fn redeem_same_id_twice_is_already_applied_and_xp_spent_once() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let wish = xp_wish(10);
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r1").unwrap_err();
        assert!(matches!(e, DbOpError::AlreadyApplied));
        assert_eq!(xp_sum(&conn, day), 40);
    }

    #[test]
    fn redeem_retry_same_id_after_insufficient_balance_returns_already_applied() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let wish = xp_wish(40);
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r1").unwrap_err();
        assert_eq!(e, DbOpError::AlreadyApplied);
        assert_eq!(xp_sum(&conn, day), 10);
    }

    #[test]
    fn redeem_insufficient_second_id_leaves_xp_spent_once() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let wish = xp_wish(40);
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &wish, "咖啡", now, "r2").unwrap_err();
        assert_eq!(e, DbOpError::Rejected("insufficient".into()));
        assert_eq!(xp_sum(&conn, day), 10);
    }

    #[test]
    fn timed_redeem_writes_session_and_blocks_second() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now, "r1").unwrap();
        let ends: i64 = conn
            .query_row(
                "SELECT ends_at FROM entertainment_sessions WHERE redemption_id='r1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ends, now + 30 * 60);
        let e = redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now + 10, "r2")
            .unwrap_err();
        assert_eq!(e, DbOpError::Rejected("entertainment_in_progress".into()));
        assert_eq!(xp_sum(&conn, day), 40);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn coin_redeem_skips_session_and_expired_allows_new() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_coin:2026-09-10:1", day, 20, 0).unwrap();
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        let coin = gamelife_core::shop::Wish {
            id: "mug".into(),
            kind: gamelife_core::shop::WishKind::Coin,
            price: 5,
        };
        redeem(&mut conn, 3600, day, &coin, "杯子", now, "c1").unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now, "r1").unwrap();
        redeem(
            &mut conn,
            3600,
            day,
            &timed_xp(10),
            "视频",
            now + 30 * 60,
            "r2",
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn same_redemption_id_does_not_insert_second_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now, "r1")
            .unwrap_err();
        assert!(matches!(e, DbOpError::AlreadyApplied));
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(xp_sum(&conn, day), 40);
    }

    #[test]
    fn begin_write_tx_is_immediate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.db");
        let mut a = Connection::open(&path).unwrap();
        let mut b = Connection::open(&path).unwrap();
        a.busy_timeout(std::time::Duration::from_millis(0)).unwrap();
        b.busy_timeout(std::time::Duration::from_millis(0)).unwrap();
        migrate(&a).unwrap();
        let _tx = begin_write_tx(&mut a).unwrap();
        let err = begin_write_tx(&mut b).unwrap_err();
        assert_eq!(err, DbOpError::Busy);
    }

    #[test]
    fn overlapping_session_insert_rolls_back_when_count_exceeds_one() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let now = 1_700_000_000i64;
        {
            let tx = begin_write_tx(&mut conn).unwrap();
            insert_entertainment_session(&tx, "r1", "video", "视频", now, 30).unwrap();
            let e = insert_entertainment_session(&tx, "r2", "video", "视频", now, 30)
                .unwrap_err();
            assert_eq!(e, DbOpError::Rejected("entertainment_in_progress".into()));
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn timed_redeem_second_uuid_on_other_connection_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("two-conn.db");
        let mut a = open(&path).unwrap();
        migrate(&a).unwrap();
        let day = "2026-09-10";
        insert_ledger(&a, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        redeem(&mut a, 3600, day, &timed_xp(10), "视频", now, "r1").unwrap();
        let mut b = open(&path).unwrap();
        let e = redeem(&mut b, 3600, day, &timed_xp(10), "视频", now + 10, "r2")
            .unwrap_err();
        assert_eq!(e, DbOpError::Rejected("entertainment_in_progress".into()));
        assert_eq!(xp_sum(&b, day), 40);
        let n: i64 = b
            .query_row("SELECT COUNT(*) FROM entertainment_sessions", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    fn user_version(conn: &Connection) -> i32 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    fn column_names(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn migrate_adds_archived_and_sessions_on_existing_user_version_1() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE wishes (
               id TEXT PRIMARY KEY, name TEXT, kind TEXT, price INTEGER,
               duration_minutes INTEGER, notes TEXT
             );
             CREATE TABLE redemptions (
               redemption_id TEXT PRIMARY KEY, wish_id TEXT NOT NULL, ts INTEGER NOT NULL
             );
             PRAGMA user_version = 1;",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let wishes = column_names(&conn, "wishes");
        assert!(wishes.iter().any(|c| c == "archived"));
        let redemptions = column_names(&conn, "redemptions");
        assert!(redemptions.iter().any(|c| c == "name"));
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entertainment_sessions'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(user_version(&conn), 3);
    }

    #[test]
    fn migrate_v2_adds_task_tables_and_seed_wishes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 1;").unwrap();
        migrate(&conn).unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 3);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM task_lists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);
        let w: i64 = conn
            .query_row("SELECT COUNT(*) FROM wishes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(w, 6);
        migrate(&conn).unwrap();
        let w2: i64 = conn
            .query_row("SELECT COUNT(*) FROM wishes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(w2, 6);
    }

    #[test]
    fn migrate_new_db_sets_user_version_3() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn), 3);
        let samples = column_names(&conn, "samples");
        assert!(samples.iter().any(|c| c == "document_path"));
        assert!(samples.iter().any(|c| c == "bundle_id"));
        assert!(samples.iter().any(|c| c == "secure_input"));
        let slots = column_names(&conn, "slots");
        assert!(slots.iter().any(|c| c == "screenshot_path"));
        assert!(slots.iter().any(|c| c == "captured_at"));
        assert!(slots.iter().any(|c| c == "capture_context_json"));
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='app_meta'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn load_active_session_tracks_timed_redeem_until_ends_at() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        redeem(&mut conn, 3600, day, &timed_xp(10), "视频", now, "r1").unwrap();
        let ends_at = now + 30 * 60;
        let during = load_active_session(&conn, now + 100).unwrap();
        assert_eq!(
            during,
            Some(("视频".into(), ends_at, ends_at - (now + 100)))
        );
        assert_eq!(load_active_session(&conn, ends_at).unwrap(), None);
        assert_eq!(load_active_session(&conn, ends_at + 1).unwrap(), None);
    }

    #[test]
    fn insert_and_archive_wish_hides_from_active_list() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_wish(&conn, "w1", "视频", "xp", 10, Some(30)).unwrap();
        insert_wish(&conn, "w2", "咖啡", "coin", 3, None).unwrap();
        assert!(insert_wish(&conn, "w3", "视频", "xp", 10, None).is_err());
        archive_wish(&conn, "w1").unwrap();
        let active_w2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wishes WHERE id='w2' AND COALESCE(archived,0)=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let active_w1: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wishes WHERE id='w1' AND COALESCE(archived,0)=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_w2, 1);
        assert_eq!(active_w1, 0);
    }

    #[test]
    fn migrate_upgrades_legacy_schema_without_copying_path() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r"
            CREATE TABLE samples (
              ts INTEGER PRIMARY KEY,
              day TEXT NOT NULL,
              app TEXT, title TEXT, url TEXT, path TEXT,
              idle_seconds INTEGER, locked INTEGER, paused INTEGER
            );
            CREATE TABLE slots (
              day TEXT NOT NULL,
              slot_start INTEGER NOT NULL,
              PRIMARY KEY (day, slot_start)
            );
            ",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, app, title, path, idle_seconds, locked, paused)
             VALUES (1, '2026-09-11', 'Cursor', 't', '/old/screenshot.jpg', 0, 0, 0)",
            [],
        )
        .unwrap();
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn), 3);
        let path: String = conn
            .query_row("SELECT path FROM samples WHERE ts=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path, "/old/screenshot.jpg");
        let doc: Option<String> = conn
            .query_row("SELECT document_path FROM samples WHERE ts=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(doc, None);
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn), 3);
    }

    #[test]
    fn migrate_creates_monitor_tables_and_version_3() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 3);
        conn.execute(
            "INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at)
             VALUES ('tt-1','p','t','mainline',1,2,3)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES ('2026-09-13','Cursor','',1,0,15,0,0,0,0,0,0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO host_day_stats (day, host, samples, core, support, admin, side, distraction)
             VALUES ('2026-09-13','arxiv.org',1,15,0,0,0,0)",
            [],
        )
        .unwrap();
    }

    /// Phase-two tagging. Every table that can be merged carries the id of the
    /// machine that produced the row, so a merge can tell two devices' rows
    /// apart. The id is minted once and never changes on later migrations —
    /// changing it would orphan the device's remote directory.
    #[test]
    fn migrate_tags_synced_tables_with_this_device_id() {
        const TAGGED: &[&str] = &[
            "slots",
            "ledger",
            "samples",
            "app_day_stats",
            "host_day_stats",
            "days",
            "policy_versions",
            "misclassification_reports",
        ];

        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // Rows written by phase one, before the tag column existed.
        conn.execute(
            "INSERT INTO slots (day, slot_start, category) VALUES ('2026-09-13', 1, 'core')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta)
             VALUES ('slot:2026-09-13:1', '2026-09-13', 1, 5, 3)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, app, title) VALUES (1, '2026-09-13', 'Cursor', 'secret')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES ('2026-09-13','Cursor','',1,0,15,0,0,0,0,0,0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO host_day_stats (day, host, samples, core, support, admin, side, distraction)
             VALUES ('2026-09-13','arxiv.org',1,15,0,0,0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES ('2026-09-13', 1, 'ok')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO policy_versions (id, json, created_at) VALUES (1, '{}', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO misclassification_reports (id, day, slot_start, note, ts)
             VALUES (1, '2026-09-13', 1, 'n', 1)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        // Adding a column is not a version-semantics change.
        assert_eq!(user_version(&conn), 3);

        let id: String = conn
            .query_row("SELECT value FROM app_meta WHERE key='device_id'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(id.len(), 32, "device_id should be 16 random bytes in hex");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));

        for table in TAGGED {
            assert!(
                column_names(&conn, table).iter().any(|c| c == "device_id"),
                "{table} has no device_id column"
            );
            let tagged: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE device_id = ?1"),
                    params![id],
                    |r| r.get(0),
                )
                .unwrap();
            let total: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert!(total > 0, "{table} was not seeded by this test");
            assert_eq!(tagged, total, "{table} has untagged rows");
        }

        // Rows written *after* the migration are tagged too — by the trigger,
        // not by the insert site, so a future insert path cannot forget.
        conn.execute(
            "INSERT INTO slots (day, slot_start, category) VALUES ('2026-09-14', 2, 'core')",
            [],
        )
        .unwrap();
        let fresh: String = conn
            .query_row(
                "SELECT device_id FROM slots WHERE day='2026-09-14' AND slot_start=2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fresh, id);

        // An explicit tag is respected rather than overwritten: that is what
        // lets a restored snapshot keep another device's attribution.
        conn.execute(
            "INSERT INTO slots (day, slot_start, device_id) VALUES ('2026-09-14', 3, 'other')",
            [],
        )
        .unwrap();
        let other: String = conn
            .query_row(
                "SELECT device_id FROM slots WHERE day='2026-09-14' AND slot_start=3",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(other, "other");

        // Re-migrating must not mint a second identity.
        migrate(&conn).unwrap();
        let again: String = conn
            .query_row("SELECT value FROM app_meta WHERE key='device_id'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(again, id);
    }
}
