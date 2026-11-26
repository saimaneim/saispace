-- Consolidation: a memory absorbed into another keeps its row (history,
-- get by id) but leaves the session index, search and review panels.
ALTER TABLE memories ADD COLUMN superseded_by BLOB NULL REFERENCES memories(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_memories_superseded_by ON memories(superseded_by);

-- Connections between memories, so an agent follows links instead of
-- searching again (cheaper than another search round trip).
CREATE TABLE IF NOT EXISTS memory_links (
    from_id BLOB NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    to_id BLOB NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (from_id, to_id, relation),
    CHECK (from_id <> to_id)
);
CREATE INDEX IF NOT EXISTS idx_memory_links_to ON memory_links(to_id);

-- Archive session summaries beyond the newest 3 per project, as saving a
-- new one does from now on. Pinned summaries stay.
WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER w AS rn,
           FIRST_VALUE(id) OVER w AS newest
    FROM memories
    WHERE kind = 'session_summary' AND pinned = 0 AND superseded_by IS NULL
    WINDOW w AS (
        PARTITION BY COALESCE(hex(project_id), 'name:' || project_name)
        ORDER BY updated_at DESC, created_at DESC, id DESC
    )
)
UPDATE memories
SET superseded_by = (SELECT newest FROM ranked WHERE ranked.id = memories.id)
WHERE id IN (SELECT id FROM ranked WHERE rn > 3);
