-- Temporary facts ("quota exhausted until X"): a memory with an expiry shows
-- under "Estado temporal" while valid and leaves the context once past.
CREATE TABLE IF NOT EXISTS memory_expiry (
    memory_id TEXT PRIMARY KEY NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_expiry_at ON memory_expiry(expires_at);
