CREATE TABLE IF NOT EXISTS tech_rules (
    id TEXT PRIMARY KEY NOT NULL,
    technology TEXT NOT NULL,
    version_req TEXT NOT NULL DEFAULT '',
    rule TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (technology, version_req)
);

CREATE INDEX IF NOT EXISTS idx_tech_rules_technology ON tech_rules(technology);

CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    kind UNINDEXED,
    ref_id UNINDEXED,
    project_id UNINDEXED,
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3'
);

INSERT INTO search_index (kind, ref_id, project_id, title, body)
SELECT 'project', id, id, name, context_notes FROM projects;

INSERT INTO search_index (kind, ref_id, project_id, title, body)
SELECT 'task', id, project_id, title, '' FROM tasks;

INSERT INTO search_index (kind, ref_id, project_id, title, body)
SELECT 'tech_rule', id, NULL,
       CASE WHEN version_req = '' THEN technology ELSE technology || ' ' || version_req END,
       rule
FROM tech_rules;

CREATE TRIGGER IF NOT EXISTS search_projects_ai AFTER INSERT ON projects BEGIN
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('project', new.id, new.id, new.name, new.context_notes);
END;

CREATE TRIGGER IF NOT EXISTS search_projects_au AFTER UPDATE OF name, context_notes ON projects BEGIN
    DELETE FROM search_index WHERE kind = 'project' AND ref_id = old.id;
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('project', new.id, new.id, new.name, new.context_notes);
END;

CREATE TRIGGER IF NOT EXISTS search_projects_ad AFTER DELETE ON projects BEGIN
    DELETE FROM search_index WHERE kind = 'project' AND ref_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS search_tasks_ai AFTER INSERT ON tasks BEGIN
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('task', new.id, new.project_id, new.title, '');
END;

CREATE TRIGGER IF NOT EXISTS search_tasks_au AFTER UPDATE OF title ON tasks BEGIN
    DELETE FROM search_index WHERE kind = 'task' AND ref_id = old.id;
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('task', new.id, new.project_id, new.title, '');
END;

CREATE TRIGGER IF NOT EXISTS search_tasks_ad AFTER DELETE ON tasks BEGIN
    DELETE FROM search_index WHERE kind = 'task' AND ref_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS search_tech_rules_ai AFTER INSERT ON tech_rules BEGIN
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('tech_rule', new.id, NULL,
            CASE WHEN new.version_req = '' THEN new.technology ELSE new.technology || ' ' || new.version_req END,
            new.rule);
END;

CREATE TRIGGER IF NOT EXISTS search_tech_rules_au AFTER UPDATE OF technology, version_req, rule ON tech_rules BEGIN
    DELETE FROM search_index WHERE kind = 'tech_rule' AND ref_id = old.id;
    INSERT INTO search_index (kind, ref_id, project_id, title, body)
    VALUES ('tech_rule', new.id, NULL,
            CASE WHEN new.version_req = '' THEN new.technology ELSE new.technology || ' ' || new.version_req END,
            new.rule);
END;

CREATE TRIGGER IF NOT EXISTS search_tech_rules_ad AFTER DELETE ON tech_rules BEGIN
    DELETE FROM search_index WHERE kind = 'tech_rule' AND ref_id = old.id;
END;
