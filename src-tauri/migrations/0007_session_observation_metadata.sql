ALTER TABLE sessions ADD COLUMN minecraft_version TEXT;
ALTER TABLE sessions ADD COLUMN loader TEXT;
ALTER TABLE sessions ADD COLUMN utc_offset_minutes INTEGER
    CHECK (utc_offset_minutes IS NULL OR utc_offset_minutes BETWEEN -1080 AND 1080);
