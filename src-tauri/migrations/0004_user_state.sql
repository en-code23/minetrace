CREATE TABLE corrections (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    patch_json TEXT NOT NULL CHECK (json_valid(patch_json)),
    previous_json TEXT CHECK (previous_json IS NULL OR json_valid(previous_json)),
    created_at_ms INTEGER NOT NULL,
    undone_at_ms INTEGER
) STRICT;

CREATE INDEX idx_corrections_entity
    ON corrections(entity_type, entity_id, created_at_ms DESC);

CREATE TABLE session_user_state (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    ignored INTEGER NOT NULL DEFAULT 0 CHECK (ignored IN (0, 1)),
    note TEXT,
    has_started_override INTEGER NOT NULL DEFAULT 0 CHECK (has_started_override IN (0, 1)),
    started_override_utc_ms INTEGER,
    has_ended_override INTEGER NOT NULL DEFAULT 0 CHECK (has_ended_override IN (0, 1)),
    ended_override_utc_ms INTEGER,
    destination_kind_override TEXT,
    destination_id_override TEXT,
    updated_at_ms INTEGER NOT NULL,
    CHECK (has_started_override = 1 OR started_override_utc_ms IS NULL),
    CHECK (has_ended_override = 1 OR ended_override_utc_ms IS NULL)
) STRICT;

