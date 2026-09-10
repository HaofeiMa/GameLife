use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::db_error::{map_rusqlite, DbOpError};
use gamelife_core::shop::{validate_redeem, Wish, WishKind};

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
  idle_seconds INTEGER, locked INTEGER, paused INTEGER
);
CREATE TABLE IF NOT EXISTS slots (
  day TEXT NOT NULL,
  slot_start INTEGER NOT NULL,
  quest_version_id INTEGER,
  policy_version_id INTEGER,
  capture_scheduled_at INTEGER,
  capture_status TEXT,
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
  id TEXT PRIMARY KEY, name TEXT, kind TEXT, price INTEGER, duration_minutes INTEGER, notes TEXT
);
CREATE TABLE IF NOT EXISTS redemptions (
  redemption_id TEXT PRIMARY KEY,
  wish_id TEXT NOT NULL,
  ts INTEGER NOT NULL
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
";

pub fn open(path: &Path) -> Result<Connection, DbOpError> {
    let conn = Connection::open(path).map_err(map_rusqlite)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(map_rusqlite)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<(), DbOpError> {
    conn.execute_batch(SCHEMA).map_err(map_rusqlite)?;
    Ok(())
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

pub fn redeem(
    conn: &mut Connection,
    credited_today: i64,
    day: &str,
    wish: &Wish,
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
    validate_redeem(credited_today, coin, xp, wish)
        .map_err(|_| DbOpError::Fatal("redeem".into()))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    tx.execute(
        "INSERT INTO redemptions (redemption_id, wish_id, ts) VALUES (?1,?2,?3)",
        params![redemption_id, wish.id, now],
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

    #[test]
    fn redeem_same_id_twice_is_already_applied_and_xp_spent_once() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let wish = xp_wish(10);
        redeem(&mut conn, 3600, day, &wish, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &wish, "r1").unwrap_err();
        assert!(matches!(e, DbOpError::AlreadyApplied));
        assert_eq!(xp_sum(&conn, day), 40);
    }

    #[test]
    fn redeem_insufficient_second_id_leaves_xp_spent_once() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let wish = xp_wish(40);
        redeem(&mut conn, 3600, day, &wish, "r1").unwrap();
        let e = redeem(&mut conn, 3600, day, &wish, "r2").unwrap_err();
        assert!(matches!(e, DbOpError::Fatal(_)));
        assert_eq!(xp_sum(&conn, day), 10);
    }
}
