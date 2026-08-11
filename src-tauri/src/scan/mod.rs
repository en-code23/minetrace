//! Incremental source inventory and durable scan lifecycle contracts.
//!
//! This module does not interpret log contents. It produces immutable file
//! fingerprints and source-generation identities for the parser layer.

#![allow(dead_code)]

mod error;
mod fingerprint;
mod inventory;
mod model;

pub(crate) use error::ScanError;
#[allow(unused_imports)]
pub(crate) use fingerprint::{
    FingerprintOptions, create_verified_file_snapshot_with_control, fingerprint_inventory,
    fingerprint_log, fingerprint_log_with_previous_size,
    fingerprint_log_with_previous_size_and_control, open_log_read_only_no_follow,
};
#[cfg(test)]
pub(crate) use inventory::inventory_logs;
pub(crate) use inventory::{InventoryOptions, inventory_logs_with_control};
#[allow(unused_imports)]
pub(crate) use model::{
    DurableScanSnapshot, FileDecision, FileFingerprint, FingerprintedLog, InventoryReport,
    InventoryWarning, InventoryWarningKind, LogCandidate, LogFileKind, ParserStamp,
    PromotionSummary, RollbackKind, ScanMessageSeverity, ScanMode, ScanRun, ScanSnapshot,
    ScanState, SourceParseStatus, StageInventoryResult, StageSummary, StagedSource,
    StoredScanMessage,
};
