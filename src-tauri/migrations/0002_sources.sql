CREATE TABLE source_paths (
    id TEXT PRIMARY KEY,
    location_id TEXT NOT NULL REFERENCES scan_locations(id) ON DELETE CASCADE,
    instance_id TEXT REFERENCES instances(id) ON DELETE SET NULL,
    relative_path_key BLOB NOT NULL,
    relative_path_display TEXT NOT NULL,
    kind TEXT NOT NULL,
    presence TEXT NOT NULL DEFAULT 'present' CHECK (presence IN ('present', 'missing', 'unreadable')),
    current_revision_id TEXT,
    last_seen_scan_id TEXT REFERENCES scan_runs(id) ON DELETE SET NULL,
    UNIQUE (location_id, relative_path_key)
) STRICT;

CREATE TABLE source_revisions (
    id TEXT PRIMARY KEY,
    source_path_id TEXT NOT NULL REFERENCES source_paths(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation > 0),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_at_ms INTEGER NOT NULL,
    birthtime_ms INTEGER,
    platform_file_id BLOB,
    prefix_hash BLOB,
    full_hash BLOB,
    parser_name TEXT NOT NULL,
    parser_revision INTEGER NOT NULL CHECK (parser_revision > 0),
    parsed_offset INTEGER NOT NULL DEFAULT 0 CHECK (parsed_offset >= 0),
    parser_checkpoint_json TEXT CHECK (parser_checkpoint_json IS NULL OR json_valid(parser_checkpoint_json)),
    parse_status TEXT NOT NULL CHECK (parse_status IN ('pending', 'parsed', 'warning', 'failed', 'superseded')),
    created_at_ms INTEGER NOT NULL,
    UNIQUE (source_path_id, generation)
) STRICT;

CREATE INDEX idx_source_revisions_path
    ON source_revisions(source_path_id, generation DESC);

CREATE TABLE scan_file_results (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    source_path_id TEXT NOT NULL REFERENCES source_paths(id) ON DELETE CASCADE,
    source_revision_id TEXT REFERENCES source_revisions(id) ON DELETE SET NULL,
    decision TEXT NOT NULL CHECK (decision IN ('new', 'changed', 'unchanged', 'reparse', 'skipped')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'parsed', 'warning', 'failed', 'cancelled')),
    bytes_read INTEGER NOT NULL DEFAULT 0 CHECK (bytes_read >= 0),
    duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (duration_ms >= 0),
    error_code TEXT,
    PRIMARY KEY (scan_id, source_path_id)
) STRICT;

CREATE TABLE scan_staged_files (
    scan_id TEXT NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    stage_key TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    PRIMARY KEY (scan_id, stage_key)
) STRICT;

