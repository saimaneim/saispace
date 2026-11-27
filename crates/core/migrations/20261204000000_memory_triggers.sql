-- Triggers bring a lesson back on their own: an error text or command
-- fragment seen in a tool call, or `env:<host>` for the agent's host.
CREATE TABLE IF NOT EXISTS memory_triggers (
    memory_id BLOB NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    pattern TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (memory_id, pattern)
);
