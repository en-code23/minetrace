//! Typed evidence emitted by parsers. Scanner implementation lands in the next slice.

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    GameStarted,
    VersionObserved,
    ServerJoined,
    WorldJoined,
    Disconnected,
    CleanShutdown,
    Crash,
}
