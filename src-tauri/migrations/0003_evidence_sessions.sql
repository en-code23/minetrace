CREATE TABLE evidence_events (
    id TEXT PRIMARY KEY,
    source_revision_id TEXT NOT NULL REFERENCES source_revisions(id) ON DELETE CASCADE,
    event_order INTEGER NOT NULL CHECK (event_order >= 0),
    line_start INTEGER,
    line_end INTEGER,
    byte_start INTEGER,
    byte_end INTEGER,
    kind TEXT NOT NULL,
    observed_local TEXT,
    occurred_at_utc_ms INTEGER,
    utc_offset_minutes INTEGER,
    timezone_id TEXT,
    timestamp_origin TEXT NOT NULL,
    confidence_score INTEGER NOT NULL CHECK (confidence_score BETWEEN 0 AND 100),
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    event_key BLOB NOT NULL,
    UNIQUE (source_revision_id, event_key)
) STRICT;

CREATE INDEX idx_evidence_source_order
    ON evidence_events(source_revision_id, event_order);

CREATE INDEX idx_evidence_occurred_at
    ON evidence_events(occurred_at_utc_ms);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    started_at_utc_ms INTEGER NOT NULL,
    ended_at_utc_ms INTEGER,
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds >= 0),
    exit_kind TEXT NOT NULL CHECK (exit_kind IN ('clean', 'crash', 'forced', 'unknown')),
    confidence_score INTEGER NOT NULL CHECK (confidence_score BETWEEN 0 AND 100),
    confidence_label TEXT NOT NULL CHECK (confidence_label IN ('verified', 'high', 'partial', 'unknown')),
    confidence_model_revision INTEGER NOT NULL CHECK (confidence_model_revision > 0),
    reconstruction_revision INTEGER NOT NULL CHECK (reconstruction_revision > 0),
    canonical_key BLOB NOT NULL UNIQUE,
    timezone_id TEXT
) STRICT;

CREATE INDEX idx_sessions_started_at ON sessions(started_at_utc_ms DESC);
CREATE INDEX idx_sessions_instance ON sessions(instance_id, started_at_utc_ms DESC);
CREATE INDEX idx_sessions_confidence ON sessions(confidence_label);

CREATE TABLE session_evidence (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    evidence_event_id TEXT NOT NULL REFERENCES evidence_events(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('start', 'end', 'version', 'destination', 'exit', 'supporting')),
    PRIMARY KEY (session_id, evidence_event_id, role)
) STRICT;

CREATE TABLE session_sources (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    source_revision_id TEXT NOT NULL REFERENCES source_revisions(id) ON DELETE CASCADE,
    relation TEXT NOT NULL CHECK (relation IN ('primary', 'supporting', 'duplicate')),
    PRIMARY KEY (session_id, source_revision_id)
) STRICT;

CREATE TABLE servers (
    id TEXT PRIMARY KEY,
    canonical_address TEXT NOT NULL UNIQUE,
    original_address TEXT NOT NULL,
    display_name TEXT,
    first_seen_at_ms INTEGER,
    last_seen_at_ms INTEGER
) STRICT;

CREATE TABLE activity_segments (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('server', 'world', 'menu', 'unknown')),
    server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
    world_id TEXT,
    started_at_utc_ms INTEGER,
    ended_at_utc_ms INTEGER,
    confidence_score INTEGER NOT NULL CHECK (confidence_score BETWEEN 0 AND 100)
) STRICT;

CREATE INDEX idx_activity_segments_session ON activity_segments(session_id);
CREATE INDEX idx_activity_segments_server ON activity_segments(server_id);

CREATE TABLE scan_staged_evidence (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    stage_key TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    PRIMARY KEY (scan_id, stage_key)
) STRICT;

CREATE TABLE scan_staged_sessions (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    stage_key TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    PRIMARY KEY (scan_id, stage_key)
) STRICT;

