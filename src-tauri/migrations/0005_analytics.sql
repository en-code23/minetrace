CREATE TABLE session_day_slices (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    client_runtime_seconds INTEGER NOT NULL CHECK (client_runtime_seconds >= 0),
    PRIMARY KEY (session_id, local_date, timezone_id)
) STRICT;

CREATE INDEX idx_session_day_slices_date
    ON session_day_slices(local_date, timezone_id);

CREATE TABLE daily_unique_runtime (
    local_date TEXT NOT NULL,
    timezone_id TEXT NOT NULL,
    unique_runtime_seconds INTEGER NOT NULL CHECK (unique_runtime_seconds >= 0),
    dataset_revision INTEGER NOT NULL CHECK (dataset_revision >= 0),
    PRIMARY KEY (local_date, timezone_id)
) STRICT;

