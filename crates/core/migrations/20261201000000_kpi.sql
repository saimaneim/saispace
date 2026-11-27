CREATE TABLE IF NOT EXISTS kpi_snapshots (
    day TEXT NOT NULL,
    scope TEXT NOT NULL,
    metric TEXT NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (day, scope, metric)
);

CREATE INDEX IF NOT EXISTS idx_kpi_snapshots_scope_day
    ON kpi_snapshots(scope, day DESC);
