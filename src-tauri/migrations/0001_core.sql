CREATE TABLE dataset_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    confidence_model_revision INTEGER NOT NULL DEFAULT 1 CHECK (confidence_model_revision > 0),
    parser_bundle_revision INTEGER NOT NULL DEFAULT 1 CHECK (parser_bundle_revision > 0),
    updated_at_ms INTEGER NOT NULL
) STRICT;

INSERT INTO dataset_state (
    id,
    revision,
    confidence_model_revision,
    parser_bundle_revision,
    updated_at_ms
) VALUES (1, 0, 1, 1, 0);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE scan_locations (
    id TEXT PRIMARY KEY,
    origin TEXT NOT NULL CHECK (origin IN ('automatic', 'custom')),
    adapter_kind TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('windows', 'macos', 'linux')),
    path_key BLOB NOT NULL UNIQUE,
    path_display TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    validation_score INTEGER NOT NULL CHECK (validation_score BETWEEN 0 AND 100),
    status TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available', 'missing', 'unreadable', 'archived')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE launcher_installations (
    id TEXT PRIMARY KEY,
    location_id TEXT NOT NULL REFERENCES scan_locations(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    display_name TEXT NOT NULL,
    confidence_score INTEGER NOT NULL CHECK (confidence_score BETWEEN 0 AND 100),
    first_seen_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_launcher_installations_location
    ON launcher_installations(location_id);

CREATE TABLE instances (
    id TEXT PRIMARY KEY,
    installation_id TEXT REFERENCES launcher_installations(id) ON DELETE SET NULL,
    location_id TEXT NOT NULL REFERENCES scan_locations(id) ON DELETE CASCADE,
    relative_path_key BLOB NOT NULL,
    relative_path_display TEXT NOT NULL,
    name TEXT NOT NULL,
    minecraft_version TEXT,
    loader TEXT,
    loader_version TEXT,
    confidence_score INTEGER NOT NULL CHECK (confidence_score BETWEEN 0 AND 100),
    first_seen_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL,
    UNIQUE (location_id, relative_path_key)
) STRICT;

CREATE INDEX idx_instances_installation ON instances(installation_id);

CREATE TABLE scan_runs (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode IN ('quick', 'standard', 'deep')),
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'paused', 'completed', 'cancelled', 'failed', 'interrupted')),
    phase TEXT NOT NULL,
    requested_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    finished_at_ms INTEGER,
    counters_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(counters_json)),
    error_code TEXT,
    dataset_revision_before INTEGER NOT NULL,
    dataset_revision_after INTEGER
) STRICT;

CREATE INDEX idx_scan_runs_requested_at ON scan_runs(requested_at_ms DESC);

CREATE TABLE scan_messages (
    id INTEGER PRIMARY KEY,
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'error')),
    code TEXT NOT NULL,
    entity_ref TEXT,
    redacted_message TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_scan_messages_scan ON scan_messages(scan_id, severity);

