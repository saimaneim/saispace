CREATE TABLE IF NOT EXISTS project_technologies (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    ecosystem TEXT NOT NULL,
    source TEXT NOT NULL,
    docs_url TEXT NOT NULL,
    detected_at TEXT NOT NULL,
    UNIQUE (project_id, ecosystem, name)
);

CREATE INDEX IF NOT EXISTS idx_project_technologies_project_id ON project_technologies(project_id);
