//! Session invariants are deliberately independent from Tauri and SQLite.

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    Clean,
    Crash,
    Forced,
    Unknown,
}
