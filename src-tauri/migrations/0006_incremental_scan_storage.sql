CREATE UNIQUE INDEX idx_scan_runs_single_active
    ON scan_runs ((1))
    WHERE state IN ('queued', 'running', 'paused');

CREATE TABLE scan_staged_locations (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    location_id TEXT NOT NULL REFERENCES scan_locations(id) ON DELETE CASCADE,
    instance_id TEXT REFERENCES instances(id) ON DELETE CASCADE,
    scope_key TEXT NOT NULL,
    staged_at_ms INTEGER NOT NULL,
    PRIMARY KEY (scan_id, location_id, scope_key)
) STRICT;

CREATE TABLE scan_staged_sources (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    location_id TEXT NOT NULL REFERENCES scan_locations(id) ON DELETE CASCADE,
    instance_id TEXT REFERENCES instances(id) ON DELETE SET NULL,
    source_path_id TEXT NOT NULL,
    source_revision_id TEXT NOT NULL,
    relative_path_key BLOB NOT NULL,
    relative_path_display TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('log', 'compressed_log')),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_at_ms INTEGER NOT NULL,
    birthtime_ms INTEGER,
    prefix_hash BLOB NOT NULL,
    full_hash BLOB NOT NULL,
    parser_name TEXT NOT NULL,
    parser_revision INTEGER NOT NULL CHECK (parser_revision > 0),
    decision TEXT NOT NULL CHECK (decision IN ('new', 'appended', 'replaced', 'unchanged', 'reparse')),
    parse_status TEXT NOT NULL DEFAULT 'pending' CHECK (parse_status IN ('pending', 'parsed', 'warning', 'failed')),
    parse_error_code TEXT,
    generation INTEGER NOT NULL CHECK (generation > 0),
    staged_at_ms INTEGER NOT NULL,
    PRIMARY KEY (scan_id, location_id, relative_path_key)
) STRICT;

CREATE INDEX idx_scan_staged_sources_scan
    ON scan_staged_sources(scan_id, decision);
