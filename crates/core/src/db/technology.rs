use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{DetectedTechnology, Technology};
use crate::tech::detect_technologies;
use chrono::Utc;
use std::path::PathBuf;
use uuid::Uuid;

impl Db {
    /// # Errors
    ///
    /// Returns an error if the technologies cannot be loaded.
    pub async fn list_technologies(&self, project_id: Uuid) -> CoreResult<Vec<Technology>> {
        let items = sqlx::query_as::<_, Technology>(
            r"
            SELECT id, project_id, name, version, ecosystem, source, docs_url, detected_at
            FROM project_technologies
            WHERE project_id = ?1
            ORDER BY ecosystem, name
            ",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(items)
    }

    /// # Errors
    ///
    /// Returns an error if the stack cannot be replaced.
    pub async fn replace_technologies(
        &self,
        project_id: Uuid,
        detected: &[DetectedTechnology],
    ) -> CoreResult<Vec<Technology>> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM project_technologies WHERE project_id = ?1")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for item in detected {
            sqlx::query(
                r"
                INSERT OR REPLACE INTO project_technologies
                    (id, project_id, name, version, ecosystem, source, docs_url, detected_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
            )
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(&item.name)
            .bind(&item.version)
            .bind(&item.ecosystem)
            .bind(&item.source)
            .bind(&item.docs_url)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.list_technologies(project_id).await
    }

    /// # Errors
    ///
    /// Returns an error if the project does not exist or its stack cannot be detected and stored.
    pub async fn sync_project_stack(&self, project_id: Uuid) -> CoreResult<Vec<Technology>> {
        let project = self
            .get_project(project_id)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "project",
                id: project_id,
            })?;
        let root = PathBuf::from(project.path);
        let detected = tokio::task::spawn_blocking(move || detect_technologies(&root)).await??;
        // Called whenever the app or an agent looks at the stack: write only
        // when the manifests actually changed.
        let current = self.list_technologies(project_id).await?;
        if same_stack(&current, &detected) {
            return Ok(current);
        }
        self.replace_technologies(project_id, &detected).await
    }
}

fn same_stack(current: &[Technology], detected: &[DetectedTechnology]) -> bool {
    let key = |name: &str, version: &str, ecosystem: &str, source: &str, docs: &str| {
        format!("{ecosystem}\u{1}{name}\u{1}{version}\u{1}{source}\u{1}{docs}")
    };
    let mut a: Vec<String> = current
        .iter()
        .map(|t| key(&t.name, &t.version, &t.ecosystem, &t.source, &t.docs_url))
        .collect();
    let mut b: Vec<String> = detected
        .iter()
        .map(|t| key(&t.name, &t.version, &t.ecosystem, &t.source, &t.docs_url))
        .collect();
    a.sort();
    b.sort();
    a == b
}
