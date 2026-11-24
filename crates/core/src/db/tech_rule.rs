use crate::db::Db;
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{TechRule, UpsertTechRuleInput};
use chrono::Utc;
use uuid::Uuid;

pub const MAX_TECHNOLOGY_CHARS: usize = 100;
pub const MAX_VERSION_CHARS: usize = 40;
pub const MAX_RULE_CHARS: usize = 500;

const RULE_COLUMNS: &str =
    "id, technology, NULLIF(version_req, '') AS version_req, rule, created_at, updated_at";

impl Db {
    pub async fn upsert_tech_rule(&self, input: UpsertTechRuleInput) -> CoreResult<TechRule> {
        let technology =
            text_with_limit("technology", &input.technology, MAX_TECHNOLOGY_CHARS)?.to_lowercase();
        let rule = text_with_limit("rule", &input.rule, MAX_RULE_CHARS)?;
        let version_req = match input.version.as_deref().map(str::trim) {
            None | Some("") => String::new(),
            Some(version) => text_with_limit("version", version, MAX_VERSION_CHARS)?,
        };
        let now = Utc::now();

        let sql = format!(
            r"
            INSERT INTO tech_rules (id, technology, version_req, rule, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            ON CONFLICT (technology, version_req)
            DO UPDATE SET rule = excluded.rule, updated_at = excluded.updated_at
            RETURNING {RULE_COLUMNS}
            "
        );
        let saved = sqlx::query_as::<_, TechRule>(&sql)
            .bind(Uuid::now_v7())
            .bind(technology)
            .bind(version_req)
            .bind(rule)
            .bind(now)
            .fetch_one(&self.pool)
            .await?;
        Ok(saved)
    }

    pub async fn list_tech_rules(&self, technology: Option<&str>) -> CoreResult<Vec<TechRule>> {
        let filter = technology.map(|t| t.trim().to_lowercase());
        let sql = format!(
            r"
            SELECT {RULE_COLUMNS}
            FROM tech_rules
            WHERE (?1 IS NULL OR technology = ?1)
            ORDER BY technology, version_req
            "
        );
        let rules = sqlx::query_as::<_, TechRule>(&sql)
            .bind(filter)
            .fetch_all(&self.pool)
            .await?;
        Ok(rules)
    }

    pub async fn delete_tech_rule(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM tech_rules WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "tech_rule",
                id,
            });
        }
        Ok(())
    }

    pub async fn tech_rules_for_project(&self, project_id: Uuid) -> CoreResult<Vec<TechRule>> {
        let stack = self.list_technologies(project_id).await?;
        let rules = self.list_tech_rules(None).await?;
        Ok(rules.into_iter().filter(|r| r.matches(&stack)).collect())
    }
}
