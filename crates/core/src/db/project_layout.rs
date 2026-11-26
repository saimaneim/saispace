use crate::db::Db;
use crate::db::project::PROJECT_COLUMNS;
use crate::error::{CoreError, CoreResult};
use crate::models::Project;
use uuid::Uuid;

impl Db {
    /// Pins or unpins a project in the sidebar. Does not touch `updated_at`.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or the update fails.
    pub async fn set_project_pinned(&self, id: Uuid, pinned: bool) -> CoreResult<Project> {
        let sql =
            format!("UPDATE projects SET pinned = ?1 WHERE id = ?2 RETURNING {PROJECT_COLUMNS}");
        sqlx::query_as::<_, Project>(&sql)
            .bind(pinned)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "Project",
                id,
            })
    }

    /// Stores the manual sidebar order: `ids[0]` gets position 1, and so on.
    /// Projects missing from `ids` keep their current position.
    ///
    /// # Errors
    ///
    /// Returns an error if the update fails.
    pub async fn reorder_projects(&self, ids: &[Uuid]) -> CoreResult<()> {
        let mut tx = self.pool.begin().await?;
        for (position, id) in (1_i64..).zip(ids) {
            sqlx::query("UPDATE projects SET sort_order = ?1 WHERE id = ?2")
                .bind(position)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::models::CreateProjectInput;

    async fn seed(db: &Db, name: &str) -> uuid::Uuid {
        db.create_project(CreateProjectInput {
            name: name.into(),
            path: format!("/tmp/{name}"),
        })
        .await
        .unwrap()
        .id
    }

    fn names(projects: &[crate::models::Project]) -> Vec<&str> {
        projects.iter().map(|p| p.name.as_str()).collect()
    }

    #[tokio::test]
    async fn pinned_first_then_manual_order() {
        let db = Db::connect_in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let a = seed(&db, "a").await;
        let b = seed(&db, "b").await;
        let c = seed(&db, "c").await;

        db.reorder_projects(&[a, b, c]).await.unwrap();
        assert_eq!(names(&db.list_projects().await.unwrap()), ["a", "b", "c"]);

        let pinned = db.set_project_pinned(c, true).await.unwrap();
        assert!(pinned.pinned);
        assert_eq!(names(&db.list_projects().await.unwrap()), ["c", "a", "b"]);

        db.reorder_projects(&[c, b, a]).await.unwrap();
        assert_eq!(names(&db.list_projects().await.unwrap()), ["c", "b", "a"]);

        db.set_project_pinned(c, false).await.unwrap();
        db.reorder_projects(&[b, a, c]).await.unwrap();
        assert_eq!(names(&db.list_projects().await.unwrap()), ["b", "a", "c"]);
    }

    #[tokio::test]
    async fn pin_unknown_project_is_not_found() {
        let db = Db::connect_in_memory().await.unwrap();
        db.migrate().await.unwrap();
        assert!(
            db.set_project_pinned(uuid::Uuid::now_v7(), true)
                .await
                .is_err()
        );
    }
}
