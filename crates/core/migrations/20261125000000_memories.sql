CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    project_name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    topic_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_topic
    ON memories(project_name, topic_key) WHERE topic_key <> '';

CREATE TRIGGER IF NOT EXISTS search_memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('memory', new.id, new.project_id, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS search_memories_au AFTER UPDATE OF title, content, project_id ON memories BEGIN
    DELETE FROM search_index WHERE kind = 'memory' AND ref_id = old.id;
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('memory', new.id, new.project_id, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS search_memories_ad AFTER DELETE ON memories BEGIN
    DELETE FROM search_index WHERE kind = 'memory' AND ref_id = old.id;
END;
