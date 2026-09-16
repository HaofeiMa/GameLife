use rusqlite::ffi;
use rusqlite::Error;
use rusqlite::ErrorCode;

#[derive(Debug, PartialEq, Eq)]
pub enum DbOpError {
    AlreadyApplied,
    Busy,
    Rejected(String),
    Fatal(String),
}

pub fn map_rusqlite(err: Error) -> DbOpError {
    if let Some(sqlite_err) = err.sqlite_error() {
        match sqlite_err.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => DbOpError::Busy,
            ErrorCode::SystemIoFailure | ErrorCode::DiskFull | ErrorCode::DatabaseCorrupt => {
                DbOpError::Fatal(err.to_string())
            }
            ErrorCode::ConstraintViolation => match sqlite_err.extended_code {
                ffi::SQLITE_CONSTRAINT_UNIQUE | ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
                    DbOpError::AlreadyApplied
                }
                _ => DbOpError::Fatal(err.to_string()),
            },
            _ => DbOpError::Fatal(err.to_string()),
        }
    } else {
        DbOpError::Fatal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite_failure(code: i32) -> Error {
        Error::SqliteFailure(ffi::Error::new(code), Some("test".into()))
    }

    #[test]
    fn map_rusqlite_busy() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_BUSY));
        assert_eq!(mapped, DbOpError::Busy);
    }

    #[test]
    fn map_rusqlite_locked() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_LOCKED));
        assert_eq!(mapped, DbOpError::Busy);
    }

    #[test]
    fn map_rusqlite_fatal_ioerr() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_IOERR));
        assert!(matches!(mapped, DbOpError::Fatal(_)));
    }

    #[test]
    fn map_rusqlite_fatal_corrupt() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_CORRUPT));
        assert!(matches!(mapped, DbOpError::Fatal(_)));
    }

    #[test]
    fn map_rusqlite_unique_is_already_applied() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_CONSTRAINT_UNIQUE));
        assert_eq!(mapped, DbOpError::AlreadyApplied);
    }

    #[test]
    fn map_rusqlite_primary_key_is_already_applied() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_CONSTRAINT_PRIMARYKEY));
        assert_eq!(mapped, DbOpError::AlreadyApplied);
    }

    #[test]
    fn map_rusqlite_notnull_constraint_is_fatal() {
        let mapped = map_rusqlite(sqlite_failure(ffi::SQLITE_CONSTRAINT_NOTNULL));
        assert!(matches!(mapped, DbOpError::Fatal(_)));
    }
}
