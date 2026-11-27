use crate::error::{CoreError, CoreResult};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

pub mod activity;
pub mod goal;
pub mod import;
pub mod kpi;
pub mod memory;
pub mod memory_dupes;
pub mod memory_edit;
pub mod memory_graph;
pub mod memory_orphans;
pub mod memory_pin;
pub mod memory_stale;
pub mod memory_stats;
pub mod memory_topics;
pub mod memory_usage;
pub mod project;
pub mod project_layout;
pub mod search;
pub mod secret;
pub mod task;
pub mod tech_rule;
pub mod technology;
mod validation;

pub use memory_dupes::DuplicatePair;
pub use memory_orphans::OrphanGroup;
pub use memory_stale::StaleReport;

#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the database URL is invalid or the pool cannot connect.
    pub async fn connect(database_url: &str) -> CoreResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created or the database cannot be opened.
    pub async fn open(path: &Path) -> CoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CoreError::Validation(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be opened.
    pub async fn connect_in_memory() -> CoreResult<Self> {
        let options = SqliteConnectOptions::from_str(":memory:")?
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// # Errors
    ///
    /// Returns an error if a migration fails.
    pub async fn migrate(&self) -> CoreResult<()> {
        // The app, `sai` and the MCP share one database but update separately:
        // an older binary must keep working after a newer one migrated it.
        // Migrations are additive, so unknown applied ones are safe to skip.
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.set_ignore_missing(true);
        migrator.run(&self.pool).await?;
        Ok(())
    }

    /// Resolves a full UUID or a unique hex suffix (the 8-digit short id
    /// printed in listings) against one table.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing matches, or a suffix matches several rows.
    async fn resolve_short_id(&self, table: &str, entity: &str, raw: &str) -> CoreResult<Uuid> {
        let raw = raw.trim();
        if let Ok(id) = Uuid::parse_str(raw) {
            return Ok(id);
        }
        let hex: String = raw.chars().filter(|c| *c != '-').collect();
        if !(6..=32).contains(&hex.len()) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CoreError::Validation(format!(
                "'{raw}' is not a {entity} id: pass the full id or its last 8 hex digits"
            )));
        }
        // The table name is a code literal, never user input.
        let sql = format!("SELECT id FROM {table} WHERE lower(hex(id)) LIKE '%' || ?1 LIMIT 3");
        let ids: Vec<Uuid> = sqlx::query_scalar(&sql)
            .bind(hex.to_ascii_lowercase())
            .fetch_all(&self.pool)
            .await?;
        match ids.as_slice() {
            [id] => Ok(*id),
            [] => Err(CoreError::Validation(format!(
                "no {entity} with id ending in {hex}"
            ))),
            _ => Err(CoreError::Validation(format!(
                "id {hex} matches several {entity}s: use more digits"
            ))),
        }
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
