use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::db_error::{map_rusqlite, DbOpError};

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
}
