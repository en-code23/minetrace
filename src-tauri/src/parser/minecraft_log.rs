//! Streaming parser for Minecraft Java client logs.
//!
//! The parser deliberately accepts any [`BufRead`]. Plain files and decoded gzip
//! streams therefore share the same parsing and resource-limit path. No parser
//! behavior depends on a concrete file or decoder type.

use std::{
    borrow::Cow,
    error::Error,
    fmt,
    io::{self, BufRead},
    str,
};

use chrono::{
    DateTime, Days, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
};
use serde::{Deserialize, Serialize};

const MILLIS_PER_DAY: i64 = 86_400_000;
const MIDNIGHT_ROLLOVER_THRESHOLD_SECONDS: i64 = 12 * 60 * 60;
const CANCELLATION_CHECK_BYTES: usize = 64 * 1024;

/// Production limits applied to the decoded stream of every log, including
/// gzip input. They bound memory retained for one line/evidence set and CPU
/// spent growing diagnostic counters on hostile or corrupt input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogParseLimits {
    /// Maximum bytes accepted after decompression (or directly for plain logs).
    pub max_decompressed_bytes: u64,
    /// Maximum bytes in one decoded line, including its line ending.
    pub max_line_bytes: usize,
    /// Maximum lines parsed from one physical source revision.
    pub max_lines: u64,
    /// Maximum typed evidence events retained for one source revision.
    pub max_evidence_events: u64,
}

impl Default for LogParseLimits {
    fn default() -> Self {
        Self {
            max_decompressed_bytes: 256 * 1024 * 1024,
            max_line_bytes: 512 * 1024,
            max_lines: 5_000_000,
            max_evidence_events: 250_000,
        }
    }
}

#[derive(Debug)]
struct LogParseLimitExceeded {
    resource: &'static str,
    limit: u64,
}

impl fmt::Display for LogParseLimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Minecraft log exceeded the {} limit ({})",
            self.resource, self.limit
        )
    }
}

impl Error for LogParseLimitExceeded {}

/// Returns true only for this parser's intentional resource-limit failures,
/// keeping corrupt gzip/I/O failures distinguishable to the scan audit trail.
pub fn is_log_parse_limit_error(error: &io::Error) -> bool {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<LogParseLimitExceeded>())
        .is_some()
}

/// Context supplied by discovery/scanning for one physical log revision.
///
/// `source_order` must increase from the oldest rotation to the newest. A date
/// hint normally identifies the first observed day from an archive filename.
/// For an undated `latest.log`, the scanner marks the filesystem date as the
/// final observed day so overnight rollovers anchor backwards correctly. The
/// fixed UTC offset is intentionally explicit so this pure parser never guesses
/// the host timezone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogParseContext {
    pub source_id: String,
    pub source_order: u32,
    /// Full raw-source fingerprint supplied by the scanner. Reconstruction
    /// uses it only to prove that two physical sources are byte-identical
    /// copies; timestamp/event coincidence alone is never treated as a copy.
    #[serde(default)]
    pub source_content_hash: Option<[u8; 32]>,
    pub date_hint: Option<NaiveDate>,
    #[serde(default)]
    pub date_hint_basis: DateHintBasis,
    pub utc_offset_minutes: Option<i32>,
    pub source_end_hint: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateHintBasis {
    #[default]
    FirstObservedDay,
    FinalObservedDay,
}

impl LogParseContext {
    pub fn new(source_id: impl Into<String>, source_order: u32) -> Self {
        Self {
            source_id: source_id.into(),
            source_order,
            source_content_hash: None,
            date_hint: None,
            date_hint_basis: DateHintBasis::FirstObservedDay,
            utc_offset_minutes: None,
            source_end_hint: None,
        }
    }

    pub fn with_date_hint(mut self, date_hint: NaiveDate) -> Self {
        self.date_hint = Some(date_hint);
        self
    }

    pub fn with_source_content_hash(mut self, source_content_hash: [u8; 32]) -> Self {
        self.source_content_hash = Some(source_content_hash);
        self
    }

    pub fn with_final_date_hint(mut self, date_hint: NaiveDate) -> Self {
        self.date_hint = Some(date_hint);
        self.date_hint_basis = DateHintBasis::FinalObservedDay;
        self
    }

    pub fn with_utc_offset(mut self, utc_offset: FixedOffset) -> Self {
        self.utc_offset_minutes = Some(utc_offset.local_minus_utc() / 60);
        self
    }

    pub fn with_source_end_hint(mut self, source_end_hint: DateTime<FixedOffset>) -> Self {
        self.source_end_hint = Some(source_end_hint);
        self
    }

    fn fixed_offset(&self) -> Option<FixedOffset> {
        self.utc_offset_minutes
            .and_then(|minutes| minutes.checked_mul(60))
            .and_then(FixedOffset::east_opt)
    }

    fn initial_date(&self) -> Option<NaiveDate> {
        match self.date_hint_basis {
            DateHintBasis::FirstObservedDay => self.date_hint,
            DateHintBasis::FinalObservedDay => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampOrigin {
    LineDateTime,
    LineTimeWithDateHint,
    LineTimeOnly,
    Missing,
}

/// Timestamp as observed in a log plus normalized values when context permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceTimestamp {
    pub observed_local: Option<NaiveDateTime>,
    pub time_of_day: Option<NaiveTime>,
    pub occurred_at_utc_ms: Option<i64>,
    pub utc_offset_minutes: Option<i32>,
    pub origin: TimestampOrigin,
    /// Deterministic ordering/duration value. It is UTC milliseconds when an
    /// offset exists, naive epoch milliseconds when a date exists, and
    /// rollover-aware milliseconds from day zero for time-only input.
    pub sequence_millis: Option<i64>,
}

impl EvidenceTimestamp {
    pub fn missing() -> Self {
        Self {
            observed_local: None,
            time_of_day: None,
            occurred_at_utc_ms: None,
            utc_offset_minutes: None,
            origin: TimestampOrigin::Missing,
            sequence_millis: None,
        }
    }

    pub fn from_datetime(value: DateTime<FixedOffset>) -> Self {
        let local = value.naive_local();
        Self {
            observed_local: Some(local),
            time_of_day: Some(local.time()),
            occurred_at_utc_ms: Some(value.timestamp_millis()),
            utc_offset_minutes: Some(value.offset().local_minus_utc() / 60),
            origin: TimestampOrigin::LineDateTime,
            sequence_millis: Some(value.timestamp_millis()),
        }
    }

    pub fn comparable_millis(&self) -> Option<i64> {
        self.sequence_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectKind {
    Remote,
    Network,
    IntegratedServerStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashMarker {
    FatalLogLevel,
    UnreportedException,
    CrashReportSaved,
    CrashReportHeader,
    GameCrashed,
}

/// Typed, privacy-conscious evidence. It never retains an entire raw log line.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MinecraftLogEvent {
    GameStarted,
    VersionObserved { version: String },
    ServerJoined { address: String },
    IntegratedServerStarted { version: Option<String> },
    WorldLoaded { world_name: Option<String> },
    Disconnected { disconnect_kind: DisconnectKind },
    Stopping,
    CleanShutdown,
    Crash { marker: CrashMarker },
}

impl MinecraftLogEvent {
    pub fn tag(&self) -> EvidenceTag {
        match self {
            Self::GameStarted => EvidenceTag::GameStarted,
            Self::VersionObserved { .. } => EvidenceTag::VersionObserved,
            Self::ServerJoined { .. } => EvidenceTag::ServerJoined,
            Self::IntegratedServerStarted { .. } => EvidenceTag::IntegratedServerStarted,
            Self::WorldLoaded { .. } => EvidenceTag::WorldLoaded,
            Self::Disconnected { .. } => EvidenceTag::Disconnected,
            Self::Stopping => EvidenceTag::Stopping,
            Self::CleanShutdown => EvidenceTag::CleanShutdown,
            Self::Crash { .. } => EvidenceTag::Crash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTag {
    GameStarted,
    VersionObserved,
    ServerJoined,
    IntegratedServerStarted,
    WorldLoaded,
    Disconnected,
    Stopping,
    CleanShutdown,
    Crash,
}

/// Stable recognition rule identifier for auditability and parser migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRule {
    LoadingMinecraft,
    StartingMinecraftClientVersion,
    ForgeForMinecraftVersion,
    MinecraftVersionField,
    SettingUser,
    LwjglInitialized,
    ConnectingToServer,
    StartingIntegratedServer,
    PreparingLevel,
    PreparingStartRegion,
    DisconnectedFromServer,
    NetworkDisconnect,
    IntegratedServerStopped,
    ClientStopping,
    ShutdownComplete,
    FatalLogLevel,
    UnreportedException,
    CrashReportSaved,
    CrashReportHeader,
    GameCrashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceProvenance {
    pub source_id: String,
    pub source_order: u32,
    /// One-based line number.
    pub line_number: u64,
    /// Half-open byte range in the decoded source stream.
    pub byte_start: u64,
    pub byte_end: u64,
    pub rule: EvidenceRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEvidence {
    pub event: MinecraftLogEvent,
    pub timestamp: EvidenceTimestamp,
    pub confidence_score: u8,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogParseDiagnostics {
    /// Total bytes consumed from the decoded/decompressed source stream.
    pub decoded_bytes: u64,
    pub total_lines: u64,
    pub recognized_lines: u64,
    pub emitted_evidence: u64,
    pub malformed_utf8_lines: u64,
    pub nul_bytes_removed: u64,
    pub evidence_without_timestamp: u64,
    pub unparsed_timestamp_prefixes: u64,
    pub non_monotonic_timestamps: u64,
    pub midnight_rollovers: u64,
    pub final_line_without_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedLog {
    pub context: LogParseContext,
    pub evidence: Vec<LogEvidence>,
    pub diagnostics: LogParseDiagnostics,
}

#[derive(Debug)]
struct TimestampState {
    current_date: Option<NaiveDate>,
    last_time: Option<NaiveTime>,
    day_index: i64,
}

impl TimestampState {
    fn new(date_hint: Option<NaiveDate>) -> Self {
        Self {
            current_date: date_hint,
            last_time: None,
            day_index: 0,
        }
    }
}

#[derive(Debug)]
struct RecognizedEvent {
    event: MinecraftLogEvent,
    rule: EvidenceRule,
    confidence_score: u8,
}

/// Parse one plain or already-decoded Minecraft log stream.
#[allow(dead_code)]
pub fn parse_minecraft_log<R: BufRead>(
    reader: R,
    context: LogParseContext,
) -> io::Result<ParsedLog> {
    parse_minecraft_log_with_control(reader, context, LogParseLimits::default(), || false)
}

/// Parse a log with explicit resource limits and cooperative cancellation.
///
/// `is_cancelled` is checked before and during line reads (at least once per
/// [`CANCELLATION_CHECK_BYTES`] of decoded input), before line processing, and
/// while emitting evidence. Cancellation is reported as
/// [`io::ErrorKind::Interrupted`]; resource-limit violations use
/// [`io::ErrorKind::InvalidData`].
pub fn parse_minecraft_log_with_control<R, C>(
    mut reader: R,
    context: LogParseContext,
    limits: LogParseLimits,
    mut is_cancelled: C,
) -> io::Result<ParsedLog>
where
    R: BufRead,
    C: FnMut() -> bool,
{
    let mut evidence = Vec::new();
    let mut diagnostics = LogParseDiagnostics::default();
    let mut timestamp_state = TimestampState::new(context.initial_date());
    let mut bytes = Vec::new();
    let mut byte_offset = 0_u64;

    loop {
        let bytes_read = read_bounded_line(
            &mut reader,
            &mut bytes,
            byte_offset,
            &limits,
            &mut is_cancelled,
        )?;
        if bytes_read == 0 {
            break;
        }
        check_parse_cancelled(&mut is_cancelled)?;

        if diagnostics.total_lines >= limits.max_lines {
            return Err(parse_limit_error("line count", limits.max_lines));
        }

        diagnostics.total_lines = diagnostics.total_lines.saturating_add(1);
        let line_number = diagnostics.total_lines;
        let byte_start = byte_offset;
        let bytes_read = u64::try_from(bytes_read)
            .map_err(|_| parse_limit_error("decompressed bytes", limits.max_decompressed_bytes))?;
        byte_offset = byte_offset.checked_add(bytes_read).ok_or_else(|| {
            parse_limit_error("decompressed bytes", limits.max_decompressed_bytes)
        })?;
        let byte_end = byte_offset;
        let has_newline = bytes.ends_with(b"\n");
        if !has_newline {
            diagnostics.final_line_without_newline = true;
        }

        let content_end = bytes
            .len()
            .saturating_sub(usize::from(has_newline))
            .saturating_sub(usize::from(bytes.ends_with(b"\r\n")));
        let content = &bytes[..content_end];

        if str::from_utf8(content).is_err() {
            diagnostics.malformed_utf8_lines = diagnostics.malformed_utf8_lines.saturating_add(1);
        }

        let decoded = String::from_utf8_lossy(content);
        let nul_count = decoded.as_bytes().iter().filter(|byte| **byte == 0).count() as u64;
        diagnostics.nul_bytes_removed = diagnostics.nul_bytes_removed.saturating_add(nul_count);
        let sanitized: Cow<'_, str> = if nul_count == 0 {
            decoded
        } else {
            Cow::Owned(decoded.replace('\0', ""))
        };

        let timestamp =
            parse_timestamp(&sanitized, &context, &mut timestamp_state, &mut diagnostics);
        let recognized = recognize_events(&sanitized);

        if !recognized.is_empty() {
            diagnostics.recognized_lines = diagnostics.recognized_lines.saturating_add(1);
        }
        let recognized_count = u64::try_from(recognized.len())
            .map_err(|_| parse_limit_error("evidence event count", limits.max_evidence_events))?;
        if diagnostics
            .emitted_evidence
            .checked_add(recognized_count)
            .is_none_or(|count| count > limits.max_evidence_events)
        {
            return Err(parse_limit_error(
                "evidence event count",
                limits.max_evidence_events,
            ));
        }

        for recognized_event in recognized {
            check_parse_cancelled(&mut is_cancelled)?;
            if timestamp.origin == TimestampOrigin::Missing {
                diagnostics.evidence_without_timestamp =
                    diagnostics.evidence_without_timestamp.saturating_add(1);
            }

            evidence.push(LogEvidence {
                event: recognized_event.event,
                timestamp: timestamp.clone(),
                confidence_score: recognized_event.confidence_score,
                provenance: EvidenceProvenance {
                    source_id: context.source_id.clone(),
                    source_order: context.source_order,
                    line_number,
                    byte_start,
                    byte_end,
                    rule: recognized_event.rule,
                },
            });
            diagnostics.emitted_evidence = diagnostics.emitted_evidence.saturating_add(1);
        }
    }

    anchor_time_only_evidence_to_final_date(&mut evidence, &context, &timestamp_state);
    diagnostics.decoded_bytes = byte_offset;

    Ok(ParsedLog {
        context,
        evidence,
        diagnostics,
    })
}

fn anchor_time_only_evidence_to_final_date(
    evidence: &mut [LogEvidence],
    context: &LogParseContext,
    state: &TimestampState,
) {
    if context.date_hint_basis != DateHintBasis::FinalObservedDay {
        return;
    }
    let Some(mut final_date) = context
        .source_end_hint
        .map(|value| value.date_naive())
        .or(context.date_hint)
    else {
        return;
    };
    if let (Some(source_end), Some(last_observed_clock)) =
        (context.source_end_hint, state.last_time)
    {
        // A log can be flushed or closed just after midnight even though its
        // final line was written just before midnight. In that case the mtime's
        // calendar day is one day too new for the final observed clock.
        let clock_lead_seconds = last_observed_clock
            .signed_duration_since(source_end.time())
            .num_seconds();
        if clock_lead_seconds > MIDNIGHT_ROLLOVER_THRESHOLD_SECONDS
            && let Some(previous_date) = final_date.pred_opt()
        {
            final_date = previous_date;
        }
    }
    let final_day_index = state.day_index.max(0);
    for item in evidence {
        if item.timestamp.origin != TimestampOrigin::LineTimeOnly {
            continue;
        }
        let (Some(sequence_millis), Some(time)) =
            (item.timestamp.sequence_millis, item.timestamp.time_of_day)
        else {
            continue;
        };
        let event_day_index = sequence_millis.div_euclid(MILLIS_PER_DAY);
        let days_before_final = final_day_index.saturating_sub(event_day_index);
        let Ok(days_before_final) = u64::try_from(days_before_final) else {
            continue;
        };
        let Some(date) = final_date.checked_sub_days(Days::new(days_before_final)) else {
            continue;
        };
        item.timestamp = timestamp_from_local(
            date.and_time(time),
            context.fixed_offset(),
            TimestampOrigin::LineTimeWithDateHint,
        );
    }
}

fn read_bounded_line<R, C>(
    reader: &mut R,
    bytes: &mut Vec<u8>,
    decoded_before_line: u64,
    limits: &LogParseLimits,
    is_cancelled: &mut C,
) -> io::Result<usize>
where
    R: BufRead,
    C: FnMut() -> bool,
{
    bytes.clear();
    loop {
        check_parse_cancelled(is_cancelled)?;
        let (chunk_len, has_newline) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(bytes.len());
            }

            // A BufRead implementation may expose its entire input at once.
            // Inspecting bounded chunks keeps cancellation latency independent
            // of that implementation and avoids `read_until`-style growth.
            let inspected = &available[..available.len().min(CANCELLATION_CHECK_BYTES)];
            let (chunk_len, has_newline) = match inspected.iter().position(|byte| *byte == b'\n') {
                Some(index) => (index + 1, true),
                None => (inspected.len(), false),
            };

            let next_line_len = bytes
                .len()
                .checked_add(chunk_len)
                .ok_or_else(|| parse_limit_error("line bytes", limits.max_line_bytes as u64))?;
            if next_line_len > limits.max_line_bytes {
                return Err(parse_limit_error(
                    "line bytes",
                    limits.max_line_bytes as u64,
                ));
            }
            let next_line_len_u64 = u64::try_from(next_line_len)
                .map_err(|_| parse_limit_error("line bytes", limits.max_line_bytes as u64))?;
            let next_decoded = decoded_before_line
                .checked_add(next_line_len_u64)
                .ok_or_else(|| {
                    parse_limit_error("decompressed bytes", limits.max_decompressed_bytes)
                })?;
            if next_decoded > limits.max_decompressed_bytes {
                return Err(parse_limit_error(
                    "decompressed bytes",
                    limits.max_decompressed_bytes,
                ));
            }

            bytes.extend_from_slice(&available[..chunk_len]);
            (chunk_len, has_newline)
        };
        reader.consume(chunk_len);
        if has_newline {
            return Ok(bytes.len());
        }
    }
}

fn check_parse_cancelled<C>(is_cancelled: &mut C) -> io::Result<()>
where
    C: FnMut() -> bool,
{
    if is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Minecraft log parsing cancelled",
        ))
    } else {
        Ok(())
    }
}

fn parse_limit_error(resource: &'static str, limit: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        LogParseLimitExceeded { resource, limit },
    )
}

fn parse_timestamp(
    line: &str,
    context: &LogParseContext,
    state: &mut TimestampState,
    diagnostics: &mut LogParseDiagnostics,
) -> EvidenceTimestamp {
    let Some(prefix) = bracket_prefix(line) else {
        return EvidenceTimestamp::missing();
    };

    if let Some(local) = parse_full_datetime(prefix) {
        if let Some(previous) = state.last_time
            && local.date() == state.current_date.unwrap_or(local.date())
            && local.time() < previous
        {
            diagnostics.non_monotonic_timestamps += 1;
        }

        state.current_date = Some(local.date());
        state.last_time = Some(local.time());
        state.day_index = 0;
        return timestamp_from_local(local, context.fixed_offset(), TimestampOrigin::LineDateTime);
    }

    if let Some(time) = parse_time(prefix) {
        if let Some(previous) = state.last_time
            && time < previous
        {
            let backwards_seconds = previous.signed_duration_since(time).num_seconds();
            if backwards_seconds > MIDNIGHT_ROLLOVER_THRESHOLD_SECONDS {
                state.day_index += 1;
                diagnostics.midnight_rollovers += 1;
                if let Some(date) = state.current_date {
                    state.current_date = date.succ_opt();
                }
            } else {
                diagnostics.non_monotonic_timestamps += 1;
            }
        }

        state.last_time = Some(time);
        if let Some(date) = state.current_date {
            let local = date.and_time(time);
            let origin = if context.date_hint.is_some() {
                TimestampOrigin::LineTimeWithDateHint
            } else {
                TimestampOrigin::LineDateTime
            };
            return timestamp_from_local(local, context.fixed_offset(), origin);
        }

        return EvidenceTimestamp {
            observed_local: None,
            time_of_day: Some(time),
            occurred_at_utc_ms: None,
            utc_offset_minutes: None,
            origin: TimestampOrigin::LineTimeOnly,
            sequence_millis: Some(state.day_index * MILLIS_PER_DAY + millis_since_midnight(time)),
        };
    }

    if prefix.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        diagnostics.unparsed_timestamp_prefixes += 1;
    }

    EvidenceTimestamp::missing()
}

fn timestamp_from_local(
    local: NaiveDateTime,
    utc_offset: Option<FixedOffset>,
    origin: TimestampOrigin,
) -> EvidenceTimestamp {
    let occurred_at_utc_ms = utc_offset
        .and_then(|offset| offset.from_local_datetime(&local).single())
        .map(|value| value.timestamp_millis());
    let sequence_millis =
        Some(occurred_at_utc_ms.unwrap_or_else(|| local.and_utc().timestamp_millis()));

    EvidenceTimestamp {
        observed_local: Some(local),
        time_of_day: Some(local.time()),
        occurred_at_utc_ms,
        utc_offset_minutes: utc_offset.map(|offset| offset.local_minus_utc() / 60),
        origin,
        sequence_millis,
    }
}

fn millis_since_midnight(time: NaiveTime) -> i64 {
    i64::from(time.num_seconds_from_midnight()) * 1_000 + i64::from(time.nanosecond() / 1_000_000)
}

fn bracket_prefix(line: &str) -> Option<&str> {
    let remainder = line.strip_prefix('[')?;
    let end = remainder.find(']')?;
    Some(&remainder[..end])
}

fn parse_full_datetime(prefix: &str) -> Option<NaiveDateTime> {
    [
        "%d%b%Y %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(prefix, format).ok())
}

fn parse_time(prefix: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(prefix, "%H:%M:%S%.f").ok()
}

fn recognize_events(line: &str) -> Vec<RecognizedEvent> {
    let message = message_body(line).trim();
    let lower_message = message.to_ascii_lowercase();
    let lower_line = line.to_ascii_lowercase();
    let mut events = Vec::with_capacity(3);
    let mut emitted_start = false;

    if let Some(version) = extract_version_after(message, &lower_message, "loading minecraft ") {
        events.push(recognized(
            MinecraftLogEvent::GameStarted,
            EvidenceRule::LoadingMinecraft,
            94,
        ));
        events.push(recognized(
            MinecraftLogEvent::VersionObserved { version },
            EvidenceRule::LoadingMinecraft,
            98,
        ));
        emitted_start = true;
    } else if let Some(version) = extract_version_after(
        message,
        &lower_message,
        "starting minecraft client version ",
    ) {
        events.push(recognized(
            MinecraftLogEvent::GameStarted,
            EvidenceRule::StartingMinecraftClientVersion,
            96,
        ));
        events.push(recognized(
            MinecraftLogEvent::VersionObserved { version },
            EvidenceRule::StartingMinecraftClientVersion,
            99,
        ));
        emitted_start = true;
    } else if let Some(version) = extract_version_after(message, &lower_message, "for minecraft ")
        && lower_message.contains("forge mod loader version")
    {
        events.push(recognized(
            MinecraftLogEvent::GameStarted,
            EvidenceRule::ForgeForMinecraftVersion,
            91,
        ));
        events.push(recognized(
            MinecraftLogEvent::VersionObserved { version },
            EvidenceRule::ForgeForMinecraftVersion,
            96,
        ));
        emitted_start = true;
    } else if let Some(version) =
        extract_version_after(message, &lower_message, "minecraft version: ")
    {
        events.push(recognized(
            MinecraftLogEvent::VersionObserved { version },
            EvidenceRule::MinecraftVersionField,
            96,
        ));
    }

    if !emitted_start && lower_message.starts_with("setting user:") {
        events.push(recognized(
            MinecraftLogEvent::GameStarted,
            EvidenceRule::SettingUser,
            82,
        ));
    } else if !emitted_start
        && (lower_message.starts_with("lwjgl version:")
            || lower_message.starts_with("backend library: lwjgl version"))
    {
        events.push(recognized(
            MinecraftLogEvent::GameStarted,
            EvidenceRule::LwjglInitialized,
            72,
        ));
    }

    if let Some(address) = extract_server_address(message, &lower_message) {
        events.push(recognized(
            MinecraftLogEvent::ServerJoined { address },
            EvidenceRule::ConnectingToServer,
            95,
        ));
    }

    if lower_message.contains("starting integrated minecraft server") {
        let version = extract_version_after(message, &lower_message, "version ");
        events.push(recognized(
            MinecraftLogEvent::IntegratedServerStarted { version },
            EvidenceRule::StartingIntegratedServer,
            96,
        ));
    }

    if let Some(world_name) = extract_quoted_after(message, &lower_message, "preparing level ") {
        events.push(recognized(
            MinecraftLogEvent::WorldLoaded {
                world_name: Some(world_name),
            },
            EvidenceRule::PreparingLevel,
            94,
        ));
    } else if lower_message.contains("preparing start region for dimension") {
        events.push(recognized(
            MinecraftLogEvent::WorldLoaded { world_name: None },
            EvidenceRule::PreparingStartRegion,
            80,
        ));
    }

    if lower_message.contains("stopping integrated server") {
        events.push(recognized(
            MinecraftLogEvent::Disconnected {
                disconnect_kind: DisconnectKind::IntegratedServerStopped,
            },
            EvidenceRule::IntegratedServerStopped,
            92,
        ));
    } else if lower_message.contains("disconnected from server") {
        events.push(recognized(
            MinecraftLogEvent::Disconnected {
                disconnect_kind: DisconnectKind::Remote,
            },
            EvidenceRule::DisconnectedFromServer,
            92,
        ));
    } else if lower_message.contains("network disconnect")
        || lower_message.contains("client disconnected with reason")
    {
        events.push(recognized(
            MinecraftLogEvent::Disconnected {
                disconnect_kind: DisconnectKind::Network,
            },
            EvidenceRule::NetworkDisconnect,
            88,
        ));
    }

    if lower_message == "stopping!" || lower_message.starts_with("stopping minecraft") {
        events.push(recognized(
            MinecraftLogEvent::Stopping,
            EvidenceRule::ClientStopping,
            96,
        ));
    }

    if lower_message.starts_with("stopping worker threads")
        || lower_message.starts_with("soundsystem shutting down")
        || lower_message.starts_with("sound engine shut down")
        || lower_message == "shutdown complete"
    {
        events.push(recognized(
            MinecraftLogEvent::CleanShutdown,
            EvidenceRule::ShutdownComplete,
            94,
        ));
    }

    if lower_line.contains("---- minecraft crash report ----") {
        events.push(recognized(
            MinecraftLogEvent::Crash {
                marker: CrashMarker::CrashReportHeader,
            },
            EvidenceRule::CrashReportHeader,
            100,
        ));
    } else if lower_message.contains("this crash report has been saved to:") {
        events.push(recognized(
            MinecraftLogEvent::Crash {
                marker: CrashMarker::CrashReportSaved,
            },
            EvidenceRule::CrashReportSaved,
            100,
        ));
    } else if lower_message.contains("unreported exception thrown") {
        events.push(recognized(
            MinecraftLogEvent::Crash {
                marker: CrashMarker::UnreportedException,
            },
            EvidenceRule::UnreportedException,
            98,
        ));
    } else if lower_line.contains("/fatal]") || lower_line.contains("/fatal]:") {
        events.push(recognized(
            MinecraftLogEvent::Crash {
                marker: CrashMarker::FatalLogLevel,
            },
            EvidenceRule::FatalLogLevel,
            96,
        ));
    } else if lower_message.contains("minecraft has crashed")
        || lower_message.contains("the game crashed whilst")
    {
        events.push(recognized(
            MinecraftLogEvent::Crash {
                marker: CrashMarker::GameCrashed,
            },
            EvidenceRule::GameCrashed,
            96,
        ));
    }

    events
}

fn recognized(
    event: MinecraftLogEvent,
    rule: EvidenceRule,
    confidence_score: u8,
) -> RecognizedEvent {
    RecognizedEvent {
        event,
        rule,
        confidence_score,
    }
}

fn message_body(line: &str) -> &str {
    if let Some((_, message)) = line.rsplit_once("]: ") {
        return message;
    }

    if let Some(prefix) = bracket_prefix(line) {
        let prefix_end = prefix.len() + 2;
        return line[prefix_end..].trim_start();
    }

    line
}

fn extract_version_after(message: &str, lower_message: &str, lower_marker: &str) -> Option<String> {
    let start = lower_message.find(lower_marker)? + lower_marker.len();
    let candidate = message.get(start..)?.trim_start();
    let token = candidate
        .split_whitespace()
        .next()?
        .trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ':' | '\'' | '"' | '(' | ')' | '[' | ']'
            )
        });

    if token.is_empty() {
        None
    } else {
        Some(token.to_owned())
    }
}

fn extract_server_address(message: &str, lower_message: &str) -> Option<String> {
    const MARKER: &str = "connecting to ";
    let start = lower_message.find(MARKER)? + MARKER.len();
    let candidate = message.get(start..)?.trim();
    if candidate.is_empty() {
        return None;
    }

    let candidate = candidate
        .split(" with ")
        .next()
        .unwrap_or(candidate)
        .trim_end_matches(['.', ';']);

    if let Some((host, port)) = candidate.rsplit_once(',') {
        let host = host.trim();
        let port = port.trim();
        if !host.is_empty()
            && !port.is_empty()
            && port.chars().all(|character| character.is_ascii_digit())
        {
            return Some(format!("{host}:{port}"));
        }
    }

    candidate
        .split_whitespace()
        .next()
        .map(|address| address.trim_matches(|character| matches!(character, '\'' | '"')))
        .filter(|address| !address.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_quoted_after(message: &str, lower_message: &str, lower_marker: &str) -> Option<String> {
    let start = lower_message.find(lower_marker)? + lower_marker.len();
    let remainder = message.get(start..)?.trim_start();
    let quote = remainder.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let value = &remainder[quote.len_utf8()..];
    let end = value.find(quote)?;
    let value = value[..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        io::{BufReader, Cursor, Write},
    };

    use chrono::{FixedOffset, NaiveDate, TimeZone};
    use flate2::{Compression, read::GzDecoder, write::GzEncoder};

    use super::{
        CrashMarker, EvidenceRule, LogParseContext, LogParseLimits, MinecraftLogEvent,
        TimestampOrigin, is_log_parse_limit_error, parse_minecraft_log,
        parse_minecraft_log_with_control,
    };

    const CLEAN_LOG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/vanilla-clean.log"
    ));
    const CRASH_LOG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/forge-integrated-crash.log"
    ));
    const MALFORMED_LOG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/malformed-noise.log"
    ));

    fn context(source_id: &str, source_order: u32) -> LogParseContext {
        LogParseContext::new(source_id, source_order)
            .with_date_hint(NaiveDate::from_ymd_opt(2026, 8, 6).expect("valid test date"))
            .with_utc_offset(FixedOffset::east_opt(2 * 60 * 60).expect("valid offset"))
    }

    #[test]
    fn parses_clean_vanilla_lifecycle_and_destinations() {
        let parsed = parse_minecraft_log(BufReader::new(CLEAN_LOG), context("clean", 1))
            .expect("fixture parses");

        assert!(parsed.evidence.iter().any(|item| matches!(
            item.event,
            MinecraftLogEvent::VersionObserved { ref version } if version == "1.20.1"
        )));
        assert!(parsed.evidence.iter().any(|item| matches!(
            item.event,
            MinecraftLogEvent::ServerJoined { ref address } if address == "play.example.net:25565"
        )));
        assert!(
            parsed
                .evidence
                .iter()
                .any(|item| item.event == MinecraftLogEvent::Stopping)
        );
        assert!(
            parsed
                .evidence
                .iter()
                .any(|item| item.event == MinecraftLogEvent::CleanShutdown)
        );
        assert_eq!(parsed.diagnostics.malformed_utf8_lines, 0);
        assert_eq!(parsed.diagnostics.evidence_without_timestamp, 0);
    }

    #[test]
    fn output_is_independent_of_bufread_chunk_boundaries() {
        let direct = parse_minecraft_log(Cursor::new(CLEAN_LOG), context("chunked", 1))
            .expect("direct stream parses");
        let chunked = parse_minecraft_log(
            BufReader::with_capacity(3, Cursor::new(CLEAN_LOG)),
            context("chunked", 1),
        )
        .expect("small buffered stream parses");

        assert_eq!(direct, chunked);
    }

    #[test]
    fn parses_forge_datetime_world_and_crash_markers() {
        let parsed = parse_minecraft_log(BufReader::new(CRASH_LOG), context("crash", 2))
            .expect("fixture parses");

        assert!(parsed.evidence.iter().any(|item| matches!(
            item.event,
            MinecraftLogEvent::IntegratedServerStarted { ref version }
                if version.as_deref() == Some("1.21.1")
        )));
        assert!(parsed.evidence.iter().any(|item| matches!(
            item.event,
            MinecraftLogEvent::WorldLoaded { ref world_name }
                if world_name.as_deref() == Some("Redstone Lab")
        )));
        assert!(parsed.evidence.iter().any(|item| matches!(
            item.event,
            MinecraftLogEvent::Crash {
                marker: CrashMarker::UnreportedException
            }
        )));
        assert!(
            parsed
                .evidence
                .iter()
                .all(|item| item.timestamp.origin == TimestampOrigin::LineDateTime)
        );
    }

    #[test]
    fn accepts_lossy_and_truncated_bufread_input_without_panicking() {
        let bytes =
            b"[12:00:00] [Render thread/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[12:00:01] [Render thread/INFO]: nois\xffe\n\
[12:10:00] [Render thread/INFO]: Stopping!";
        let parsed = parse_minecraft_log(Cursor::new(bytes), context("malformed", 3))
            .expect("lossy input still parses");

        assert_eq!(parsed.diagnostics.malformed_utf8_lines, 1);
        assert!(parsed.diagnostics.final_line_without_newline);
        assert!(
            parsed
                .evidence
                .iter()
                .any(|item| item.event == MinecraftLogEvent::Stopping)
        );
    }

    #[test]
    fn malformed_timestamps_and_clock_regressions_are_diagnostic_only() {
        let parsed = parse_minecraft_log(BufReader::new(MALFORMED_LOG), context("noise", 3))
            .expect("malformed fixture parses deterministically");

        assert_eq!(parsed.diagnostics.total_lines, 6);
        assert_eq!(parsed.diagnostics.unparsed_timestamp_prefixes, 1);
        assert_eq!(parsed.diagnostics.non_monotonic_timestamps, 1);
        assert!(
            parsed
                .evidence
                .iter()
                .any(|item| item.event == MinecraftLogEvent::Stopping)
        );
    }

    #[test]
    fn rolls_time_only_logs_across_midnight() {
        let bytes = b"[23:59:58] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[00:00:03] [Render thread/INFO]: Stopping!\n";
        let parsed = parse_minecraft_log(Cursor::new(bytes), context("midnight", 4))
            .expect("midnight input parses");

        assert_eq!(parsed.diagnostics.midnight_rollovers, 1);
        let start = parsed.evidence.first().expect("start evidence");
        let end = parsed.evidence.last().expect("end evidence");
        assert!(
            end.timestamp.comparable_millis().expect("end millis")
                > start.timestamp.comparable_millis().expect("start millis")
        );
    }

    #[test]
    fn final_day_hint_anchors_an_overnight_undated_log_backwards() {
        let bytes = b"[23:59:58] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[00:00:03] [Render thread/INFO]: Stopping!\n";
        let offset = FixedOffset::east_opt(2 * 60 * 60).expect("valid offset");
        let source_end = offset
            .with_ymd_and_hms(2026, 8, 10, 0, 0, 5)
            .single()
            .expect("valid source end");
        let context = LogParseContext::new("latest", 4)
            .with_final_date_hint(source_end.date_naive())
            .with_utc_offset(offset)
            .with_source_end_hint(source_end);

        let parsed = parse_minecraft_log(Cursor::new(bytes), context).expect("overnight log");

        assert_eq!(parsed.diagnostics.midnight_rollovers, 1);
        let start = parsed.evidence.first().expect("start evidence");
        let end = parsed.evidence.last().expect("end evidence");
        assert_eq!(
            start.timestamp.observed_local.expect("start local").date(),
            NaiveDate::from_ymd_opt(2026, 8, 9).expect("start date")
        );
        assert_eq!(
            end.timestamp.observed_local.expect("end local").date(),
            NaiveDate::from_ymd_opt(2026, 8, 10).expect("end date")
        );
        assert_eq!(
            end.timestamp.occurred_at_utc_ms,
            Some(
                offset
                    .with_ymd_and_hms(2026, 8, 10, 0, 0, 3)
                    .single()
                    .expect("expected end")
                    .timestamp_millis()
            )
        );
    }

    #[test]
    fn final_day_hint_accounts_for_close_latency_across_midnight() {
        let bytes = b"[23:58:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[23:59:58] [Render thread/INFO]: Stopping!\n";
        let offset = FixedOffset::east_opt(2 * 60 * 60).expect("valid offset");
        let source_end = offset
            .with_ymd_and_hms(2026, 8, 10, 0, 0, 5)
            .single()
            .expect("valid source end");
        let context = LogParseContext::new("latest-close-latency", 5)
            .with_final_date_hint(source_end.date_naive())
            .with_utc_offset(offset)
            .with_source_end_hint(source_end);

        let parsed = parse_minecraft_log(Cursor::new(bytes), context).expect("late-night log");

        assert_eq!(parsed.diagnostics.midnight_rollovers, 0);
        let expected_date = NaiveDate::from_ymd_opt(2026, 8, 9).expect("expected date");
        assert!(parsed.evidence.iter().all(|item| {
            item.timestamp
                .observed_local
                .is_some_and(|local| local.date() == expected_date)
        }));
        assert_eq!(
            parsed
                .evidence
                .last()
                .expect("end evidence")
                .timestamp
                .occurred_at_utc_ms,
            Some(
                offset
                    .with_ymd_and_hms(2026, 8, 9, 23, 59, 58)
                    .single()
                    .expect("expected end")
                    .timestamp_millis()
            )
        );
    }

    #[test]
    fn records_source_offsets_and_rule_without_raw_line_storage() {
        let parsed = parse_minecraft_log(BufReader::new(CLEAN_LOG), context("clean", 7))
            .expect("fixture parses");
        let first = parsed.evidence.first().expect("evidence");

        assert_eq!(first.provenance.source_order, 7);
        assert_eq!(first.provenance.line_number, 1);
        assert_eq!(first.provenance.byte_start, 0);
        assert!(first.provenance.byte_end > first.provenance.byte_start);
        assert_eq!(first.provenance.rule, EvidenceRule::LoadingMinecraft);
    }

    #[test]
    fn context_end_hint_is_transportable_without_parser_side_effects() {
        let offset = FixedOffset::east_opt(2 * 60 * 60).expect("valid offset");
        let end = offset
            .with_ymd_and_hms(2026, 8, 6, 15, 1, 5)
            .single()
            .expect("valid timestamp");
        let context = context("hint", 9).with_source_end_hint(end);
        let parsed = parse_minecraft_log(Cursor::new(CLEAN_LOG), context).expect("fixture parses");

        assert_eq!(parsed.context.source_end_hint, Some(end));
    }

    #[test]
    fn rejects_an_oversized_line_before_growing_the_line_buffer_past_its_limit() {
        let mut input = vec![b'x'; 65];
        input.push(b'\n');
        let limits = LogParseLimits {
            max_decompressed_bytes: 1_024,
            max_line_bytes: 64,
            ..LogParseLimits::default()
        };

        let error = parse_minecraft_log_with_control(
            BufReader::with_capacity(8, Cursor::new(input)),
            context("oversized-line", 10),
            limits,
            || false,
        )
        .expect_err("oversized line must be rejected");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("line bytes"));
        assert!(is_log_parse_limit_error(&error));
        assert!(!is_log_parse_limit_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ordinary corrupt input",
        )));
    }

    #[test]
    fn caps_gzip_expansion_by_decompressed_bytes() {
        let expanded = vec![b'a'; 16 * 1_024];
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder
            .write_all(&expanded)
            .expect("encode repetitive body");
        let compressed = encoder.finish().expect("finish gzip body");
        assert!(compressed.len() < expanded.len());

        let limits = LogParseLimits {
            max_decompressed_bytes: 1_024,
            max_line_bytes: 32 * 1_024,
            ..LogParseLimits::default()
        };
        let decoded = BufReader::new(GzDecoder::new(Cursor::new(compressed)));
        let error = parse_minecraft_log_with_control(
            decoded,
            context("gzip-expansion", 11),
            limits,
            || false,
        )
        .expect_err("decompressed body must be capped");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("decompressed bytes"));
    }

    #[test]
    fn cancellation_is_checked_repeatedly_inside_one_long_line() {
        let input = vec![b'x'; 16 * 1_024];
        let checks = Cell::new(0_u32);
        let limits = LogParseLimits {
            max_decompressed_bytes: 32 * 1_024,
            max_line_bytes: 32 * 1_024,
            ..LogParseLimits::default()
        };
        let error = parse_minecraft_log_with_control(
            BufReader::with_capacity(64, Cursor::new(input)),
            context("cancel-long-line", 12),
            limits,
            || {
                let next = checks.get().saturating_add(1);
                checks.set(next);
                next >= 5
            },
        )
        .expect_err("cancellation must interrupt an in-progress line");

        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(checks.get(), 5);
    }

    #[test]
    fn caps_evidence_and_line_diagnostic_growth() {
        let input = b"[12:00:00] [main/INFO]: Loading Minecraft 1.20.1\n\
[12:00:01] [main/INFO]: Loading Minecraft 1.20.1\n";
        let evidence_error = parse_minecraft_log_with_control(
            Cursor::new(input),
            context("evidence-cap", 13),
            LogParseLimits {
                max_evidence_events: 3,
                ..LogParseLimits::default()
            },
            || false,
        )
        .expect_err("two two-event lines must exceed a three-event cap");
        assert!(evidence_error.to_string().contains("evidence event count"));

        let line_error = parse_minecraft_log_with_control(
            Cursor::new(b"noise\nnoise\n"),
            context("line-cap", 14),
            LogParseLimits {
                max_lines: 1,
                ..LogParseLimits::default()
            },
            || false,
        )
        .expect_err("line diagnostics must be bounded");
        assert!(line_error.to_string().contains("line count"));
    }
}
