CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT
);

CREATE TABLE IF NOT EXISTS secret_grants (
    id TEXT PRIMARY KEY NOT NULL,
    secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    status TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    decided_at TEXT,
    UNIQUE (secret_id, agent)
);

CREATE INDEX IF NOT EXISTS idx_secret_grants_status ON secret_grants(status, created_at DESC);
