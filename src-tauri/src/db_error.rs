use rusqlite::Error;
use rusqlite::ErrorCode;

#[derive(Debug, PartialEq, Eq)]
pub enum DbOpError {
    AlreadyApplied,
    Busy,
    Fatal(String),
}

pub fn map_rusqlite(err: Error) -> DbOpError {
    match err.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy) | Some(ErrorCode::DatabaseLocked) => DbOpError::Busy,
        Some(ErrorCode::SystemIoFailure)
        | Some(ErrorCode::DiskFull)
        | Some(ErrorCode::DatabaseCorrupt) => DbOpError::Fatal(err.to_string()),
        Some(ErrorCode::ConstraintViolation) => DbOpError::AlreadyApplied,
        _ => DbOpError::Fatal(err.to_string()),
    }
}
