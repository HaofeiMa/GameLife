use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::db_error::{map_rusqlite, DbOpError};
use gamelife_core::shop::{
    can_start_entertainment, entertainment_remaining_secs, has_entertainment_timer,
    validate_redeem, validate_wish, RedeemError, Wish, WishError, WishKind,
};

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
  observed_seconds INTEGER,
  used_vision INTEGER,
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
";

const TARGET_USER_VERSION: i32 = 1;

const WAVE1_COLUMNS: &[(&str, &str, &str)] = &[
    ("samples", "document_path", "TEXT"),
    ("samples", "bundle_id", "TEXT"),
    ("samples", "secure_input", "INTEGER NOT NULL DEFAULT 0"),
    ("slots", "screenshot_path", "TEXT"),
    ("slots", "captured_at", "INTEGER"),
    ("slots", "capture_context_json", "TEXT"),
];

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
    if version < TARGET_USER_VERSION {
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
    Ok(())
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

pub fn app_db_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        Path::new(&home)
            .join("Library/Application Support/GameLife/gamelife.db")
    })
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

pub fn redeem(
    conn: &mut Connection,
    credited_today: i64,
    day: &str,
    wish: &Wish,
    wish_name: &str,
    now: i64,
    redemption_id: &str,
) -> Result<(), DbOpError> {
    let tx = conn.transaction().map_err(map_rusqlite)?;
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
        let ends_at = now + duration * 60;
        tx.execute(
            "INSERT INTO entertainment_sessions (redemption_id, wish_id, name, started_at, ends_at) VALUES (?1,?2,?3,?4,?5)",
            params![redemption_id, wish.id, wish_name, now, ends_at],
        )
        .map_err(map_rusqlite)?;
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
        assert_eq!(user_version(&conn), 1);
    }

    #[test]
    fn migrate_new_db_sets_user_version_1() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn), 1);
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
    fn insert_and_archive_wish_hides_from_active_list() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_wish(&conn, "w1", "视频", "xp", 10, Some(30)).unwrap();
        insert_wish(&conn, "w2", "咖啡", "coin", 3, None).unwrap();
        assert!(insert_wish(&conn, "w3", "视频", "xp", 10, None).is_err());
        archive_wish(&conn, "w1").unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wishes WHERE COALESCE(archived,0)=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
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
        assert_eq!(user_version(&conn), 1);
        let path: String = conn
            .query_row("SELECT path FROM samples WHERE ts=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path, "/old/screenshot.jpg");
        let doc: Option<String> = conn
            .query_row("SELECT document_path FROM samples WHERE ts=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(doc, None);
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn), 1);
    }
}
