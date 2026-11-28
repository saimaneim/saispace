use crate::db::Db;
use crate::db::validation::{limited_text, required_text};
use crate::error::{CoreError, CoreResult};
use crate::models::{CreateProjectInput, Project, ProjectStatus, UpdateProjectInput};
use crate::paths::{
    canonical_project_path, detect_project_root, find_project_marker, is_auto_project_root,
    project_key,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) const PROJECT_COLUMNS: &str =
    "id, name, path, status, pinned, sort_order, created_at, updated_at";

impl Db {
    /// # Errors
    ///
    /// Returns an error if the input is invalid or the project cannot be stored.
    pub async fn create_project(&self, input: CreateProjectInput) -> CoreResult<Project> {
        let name = limited_text("name", &input.name)?;
        let path = required_text("path", &input.path)?;
        let id = Uuid::now_v7();
        let now = Utc::now();

        let project = sqlx::query_as::<_, Project>(&format!(
            r"
            INSERT INTO projects (id, name, path, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            RETURNING {PROJECT_COLUMNS}
            ",
        ))
        .bind(id)
        .bind(name)
        .bind(path)
        .bind(ProjectStatus::Active)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        self.link_orphan_memories(&project).await?;
        Ok(project)
    }

    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_project(&self, id: Uuid) -> CoreResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(&format!(
            r"
            SELECT {PROJECT_COLUMNS}
            FROM projects
            WHERE id = ?1
            ",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_project_by_path(&self, path: &str) -> CoreResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(&format!(
            r"
            SELECT {PROJECT_COLUMNS}
            FROM projects
            WHERE path = ?1
            ",
        ))
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    /// # Errors
    ///
    /// Returns an error if the projects cannot be loaded.
    pub async fn list_projects(&self) -> CoreResult<Vec<Project>> {
        let projects = sqlx::query_as::<_, Project>(&format!(
            r"
            SELECT {PROJECT_COLUMNS}
            FROM projects
            ORDER BY pinned DESC, sort_order ASC, updated_at DESC
            ",
        ))
        .fetch_all(&self.pool)
        .await?;

        Ok(projects)
    }

    /// # Errors
    ///
    /// Returns an error if the input is invalid, the project does not exist, or the update fails.
    pub async fn update_project(&self, id: Uuid, input: UpdateProjectInput) -> CoreResult<Project> {
        let name = input
            .name
            .as_deref()
            .map(|value| limited_text("name", value))
            .transpose()?;
        let path = input
            .path
            .as_deref()
            .map(|value| required_text("path", value))
            .transpose()?;
        let now = Utc::now();

        let updated = sqlx::query_as::<_, Project>(&format!(
            r"
            UPDATE projects
            SET name = COALESCE(?1, name),
                path = COALESCE(?2, path),
                status = COALESCE(?3, status),
                updated_at = ?4
            WHERE id = ?5
            RETURNING {PROJECT_COLUMNS}
            ",
        ))
        .bind(name)
        .bind(path)
        .bind(input.status)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or(CoreError::NotFound {
            entity: "Project",
            id,
        })
    }

    /// # Errors
    ///
    /// Returns an error if the project does not exist or the delete fails.
    pub async fn delete_project(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM projects WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "Project",
                id,
            });
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the projects cannot be loaded or the path cannot be inspected.
    pub async fn find_project_for_path(
        &self,
        path: &std::path::Path,
    ) -> CoreResult<Option<Project>> {
        let path = canonical_project_path(path);
        let projects = self.list_projects().await?;
        Ok(projects
            .into_iter()
            .filter(|p| path.starts_with(&p.path))
            .max_by_key(|p| p.path.len()))
    }

    /// Resolves the project that owns `path`. A registered project containing
    /// `path` wins. Otherwise the project root (nearest git root, or `path`)
    /// is registered when `register` is true, or when orphan memories already
    /// carry its name (imported from another tool). Roots such as `$HOME` or
    /// `~/Dev` are never registered.
    ///
    /// # Errors
    ///
    /// Returns an error if the project cannot be looked up or registered.
    pub async fn project_for_path(
        &self,
        path: &Path,
        register: bool,
    ) -> CoreResult<Option<Project>> {
        let path = canonical_project_path(path);
        let path = path.as_path();
        if let Some(wanted) = std::env::var("SAISPACE_PROJECT")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            let key = project_key(&wanted);
            let found = self
                .list_projects()
                .await?
                .into_iter()
                .find(|p| project_key(&p.name) == key || p.path == wanted);
            return match found {
                Some(project) => Ok(Some(project)),
                None => Err(CoreError::Validation(format!(
                    "SAISPACE_PROJECT={wanted} no coincide con ningún proyecto registrado"
                ))),
            };
        }
        if let Some((root, name)) = find_project_marker(path) {
            let root_str = root.to_string_lossy().to_string();
            if let Some(project) = self.get_project_by_path(&root_str).await? {
                return Ok(Some(project));
            }
            // The marker is an explicit choice: register even if not asked.
            return Ok(Some(
                self.create_project(CreateProjectInput {
                    name,
                    path: root_str,
                })
                .await?,
            ));
        }
        if let Some(project) = self.find_project_for_path(path).await? {
            return Ok(Some(project));
        }
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        let eligible = |dir: &Path| is_auto_project_root(dir, home.as_deref());
        let orphans: Vec<String> = self
            .orphan_project_names()
            .await?
            .iter()
            .map(|n| project_key(n))
            .collect();
        let has_orphans = |dir: &Path| {
            dir.file_name()
                .is_some_and(|n| orphans.contains(&project_key(&n.to_string_lossy())))
        };
        let git_root = detect_project_root(path);
        // Without git, a parent folder named like imported memories is the project.
        let root = if git_root.as_path() == path {
            path.ancestors()
                .take_while(|dir| eligible(dir))
                .find(|dir| has_orphans(dir))
                .unwrap_or(path)
                .to_path_buf()
        } else {
            git_root
        };
        if !eligible(&root) {
            return Ok(None);
        }
        let root_str = root.to_string_lossy().to_string();
        let projects = self.list_projects().await?;
        if projects
            .iter()
            .any(|p| Path::new(&p.path).starts_with(&root))
        {
            return Ok(None);
        }
        let name = root
            .file_name()
            .map_or_else(|| root_str.clone(), |n| n.to_string_lossy().to_string());
        if !register && !has_orphans(&root) {
            return Ok(None);
        }
        let project = self
            .create_project(CreateProjectInput {
                name,
                path: root_str,
            })
            .await?;
        Ok(Some(project))
    }

    async fn orphan_project_names(&self) -> CoreResult<Vec<String>> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT project_name FROM memories WHERE project_id IS NULL AND project_name <> ''",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Attaches memories saved under this project's name (with any spelling:
    /// `onix gestoria` matches `ONIX GESTORIA`) before the project existed.
    ///
    /// # Errors
    ///
    /// Returns an error if the memories cannot be updated.
    pub async fn link_orphan_memories(&self, project: &Project) -> CoreResult<u64> {
        let folder = Path::new(&project.path)
            .file_name()
            .map(|n| project_key(&n.to_string_lossy()));
        let keys: Vec<String> = [Some(project_key(&project.name)), folder]
            .into_iter()
            .flatten()
            .filter(|k| !k.is_empty())
            .collect();
        let mut linked = 0;
        for name in self.orphan_project_names().await? {
            if !keys.contains(&project_key(&name)) {
                continue;
            }
            linked += sqlx::query(
                "UPDATE OR IGNORE memories SET project_id = ?1, project_name = ?2 WHERE project_id IS NULL AND project_name = ?3",
            )
            .bind(project.id)
            .bind(&project.name)
            .bind(&name)
            .execute(&self.pool)
            .await?
            .rows_affected();
        }
        Ok(linked)
    }
}
