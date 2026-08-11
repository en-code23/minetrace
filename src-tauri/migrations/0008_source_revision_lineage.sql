ALTER TABLE source_revisions
ADD COLUMN change_kind TEXT NOT NULL DEFAULT 'new'
    CHECK (change_kind IN ('new', 'appended', 'replaced', 'reparse'));

CREATE INDEX idx_source_revisions_replacement_lineage
    ON source_revisions(source_path_id, change_kind, generation);
