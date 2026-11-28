-- project: knowledge of the repo, shareable with `sai sync`.
-- personal: the developer's own notes; never exported.
ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'project'
    CHECK (scope IN ('project', 'personal'));
