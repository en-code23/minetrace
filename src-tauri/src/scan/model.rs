use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanMode {
    Quick,
    Standard,
    Deep,
}

impl ScanMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "quick" => Some(Self::Quick),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanState {
    Queued,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl ScanState {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ScanRun {
    pub id: String,
    pub state: ScanState,
    pub dataset_revision_before: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanSnapshot {
    pub id: String,
    pub mode: ScanMode,
    pub state: ScanState,
    pub phase: String,
    pub dataset_revision_before: i64,
    pub dataset_revision_after: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanMessageSeverity {
    Warning,
    Error,
}

impl ScanMessageSeverity {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredScanMessage {
    pub severity: ScanMessageSeverity,
    pub code: String,
    pub entity_ref: Option<String>,
    pub redacted_message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DurableScanSnapshot {
    pub id: String,
    pub mode: ScanMode,
    pub state: ScanState,
    pub counters_json: String,
    pub error_code: Option<String>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub dataset_revision_before: i64,
    pub dataset_revision_after: Option<i64>,
    pub warning_count: u64,
    pub error_count: u64,
    pub messages: Vec<StoredScanMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogFileKind {
    Log,
    CompressedLog,
}

impl LogFileKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::CompressedLog => "compressed_log",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "log" => Some(Self::Log),
            "compressed_log" => Some(Self::CompressedLog),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LogCandidate {
    pub approved_root: PathBuf,
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub relative_path_key: Vec<u8>,
    pub kind: LogFileKind,
    pub observed_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InventoryWarningKind {
    SymlinkSkipped,
    DepthLimitReached,
    FileTooLarge,
    UnreadableEntry,
}

#[derive(Debug, Clone)]
pub(crate) struct InventoryWarning {
    pub kind: InventoryWarningKind,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct InventoryReport {
    pub root: PathBuf,
    pub candidates: Vec<LogCandidate>,
    pub warnings: Vec<InventoryWarning>,
    pub visited_entries: usize,
    pub skipped_symlinks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileFingerprint {
    pub size_bytes: u64,
    pub modified_at_ms: i64,
    pub birthtime_ms: Option<i64>,
    pub prefix_hash: [u8; 32],
    pub full_hash: [u8; 32],
    /// Hash of the new file's first `comparison_prefix_len` bytes, where the
    /// length is the previously observed generation size. This proves append
    /// semantics even when the old file was shorter than the fixed prefix.
    pub comparison_prefix_len: Option<u64>,
    pub comparison_prefix_hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub(crate) struct FingerprintedLog {
    pub candidate: LogCandidate,
    pub fingerprint: FileFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParserStamp {
    pub name: String,
    pub revision: u32,
}

impl ParserStamp {
    pub(crate) fn new(name: impl Into<String>, revision: u32) -> Self {
        Self {
            name: name.into(),
            revision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileDecision {
    New,
    Appended,
    Replaced,
    Unchanged,
    Reparse,
}

impl FileDecision {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Appended => "appended",
            Self::Replaced => "replaced",
            Self::Unchanged => "unchanged",
            Self::Reparse => "reparse",
        }
    }

    pub(crate) const fn requires_parse(self) -> bool {
        !matches!(self, Self::Unchanged)
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "new" => Some(Self::New),
            "appended" => Some(Self::Appended),
            "replaced" => Some(Self::Replaced),
            "unchanged" => Some(Self::Unchanged),
            "reparse" => Some(Self::Reparse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceParseStatus {
    Pending,
    Parsed,
    Warning,
    Failed,
}

impl SourceParseStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Parsed => "parsed",
            Self::Warning => "warning",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "parsed" => Some(Self::Parsed),
            "warning" => Some(Self::Warning),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StagedSource {
    pub instance_id: Option<String>,
    pub source_path_id: String,
    pub source_revision_id: String,
    pub relative_path: PathBuf,
    pub kind: LogFileKind,
    pub decision: FileDecision,
    pub generation: i64,
    pub fingerprint: FileFingerprint,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StageSummary {
    pub new_files: usize,
    pub appended_files: usize,
    pub replaced_files: usize,
    pub reparsed_files: usize,
    pub unchanged_files: usize,
}

impl StageSummary {
    pub(crate) fn total(&self) -> usize {
        self.new_files
            + self.appended_files
            + self.replaced_files
            + self.reparsed_files
            + self.unchanged_files
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StageInventoryResult {
    pub summary: StageSummary,
    pub sources: Vec<StagedSource>,
    /// Instances whose previously-present source paths are absent from this
    /// complete inventory scope. Their existing source-linked sessions must be
    /// protected while evidence from the still-present paths is merged.
    pub missing_source_instance_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PromotionSummary {
    pub dataset_revision: i64,
    pub new_files: usize,
    pub appended_files: usize,
    pub replaced_files: usize,
    pub reparsed_files: usize,
    pub unchanged_files: usize,
    pub missing_files: usize,
    pub missing_source_instance_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RollbackKind {
    Cancelled,
    Failed { error_code: String },
}
