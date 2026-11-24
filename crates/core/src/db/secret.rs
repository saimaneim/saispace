use super::Db;
use super::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{GrantStatus, SecretGrant, SecretMeta};
use crate::vault::Vault;
use chrono::Utc;
use uuid::Uuid;

const MAX_SECRET_BYTES: usize = 64 * 1024;
const MAX_NAME_LEN: usize = 64;

pub fn validate_secret_name(name: &str) -> CoreResult<String> {
    let name = name.trim();
    let mut chars = name.chars();
    let valid_start = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let valid_rest = chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !valid_start || !valid_rest || name.len() > MAX_NAME_LEN {
        return Err(CoreError::Validation(format!(
            "secret name must match [A-Z][A-Z0-9_]* and be at most {MAX_NAME_LEN} characters"
        )));
    }
    Ok(name.to_string())
}

fn validate_agent(agent: &str) -> CoreResult<String> {
    text_with_limit("agent", agent, MAX_NAME_LEN)
}

const GRANT_COLUMNS: &str = "g.id, g.secret_id, s.name AS secret_name, g.agent, g.status, g.reason, g.created_at, g.decided_at";

impl Db {
    pub async fn set_secret(
        &self,
        vault: &Vault,
        name: &str,
        value: &str,
        description: &str,
    ) -> CoreResult<SecretMeta> {
        let name = validate_secret_name(name)?;
        if value.is_empty() {
            return Err(CoreError::Validation(
                "secret value must not be empty".to_string(),
            ));
        }
        if value.len() > MAX_SECRET_BYTES {
            return Err(CoreError::Validation(format!(
                "secret value must be at most {MAX_SECRET_BYTES} bytes"
            )));
        }
        let description = description.trim();
        if description.chars().count() > 500 {
            return Err(CoreError::Validation(
                "description must be at most 500 characters".to_string(),
            ));
        }
        let (nonce, ciphertext) = vault.seal(&name, value.as_bytes())?;
        let now = Utc::now();
        let meta = sqlx::query_as::<_, SecretMeta>(
            r"
            INSERT INTO secrets (id, name, description, nonce, ciphertext, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(name) DO UPDATE SET
                description = CASE WHEN excluded.description = '' THEN secrets.description ELSE excluded.description END,
                nonce = excluded.nonce,
                ciphertext = excluded.ciphertext,
                updated_at = excluded.updated_at
            RETURNING id, name, description, created_at, updated_at, last_used_at
            ",
        )
        .bind(Uuid::now_v7())
        .bind(&name)
        .bind(description)
        .bind(nonce)
        .bind(ciphertext)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(meta)
    }

    pub async fn list_secrets(&self) -> CoreResult<Vec<SecretMeta>> {
        let items = sqlx::query_as::<_, SecretMeta>(
            "SELECT id, name, description, created_at, updated_at, last_used_at FROM secrets ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(items)
    }

    async fn secret_id(&self, name: &str) -> CoreResult<Uuid> {
        let name = validate_secret_name(name)?;
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM secrets WHERE name = ?1")
            .bind(&name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("secret {name} does not exist")))
    }

    pub async fn delete_secret(&self, name: &str) -> CoreResult<()> {
        let id = self.secret_id(name).await?;
        sqlx::query("DELETE FROM secrets WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn request_secret_access(
        &self,
        name: &str,
        agent: &str,
        reason: &str,
    ) -> CoreResult<SecretGrant> {
        let secret_id = self.secret_id(name).await?;
        let agent = validate_agent(agent)?;
        let reason: String = reason.trim().chars().take(500).collect();
        sqlx::query(
            r"
            INSERT INTO secret_grants (id, secret_id, agent, status, reason, created_at)
            VALUES (?1, ?2, ?3, 'pending', ?4, ?5)
            ON CONFLICT(secret_id, agent) DO UPDATE SET
                reason = CASE WHEN secret_grants.status = 'pending' AND excluded.reason <> '' THEN excluded.reason ELSE secret_grants.reason END
            ",
        )
        .bind(Uuid::now_v7())
        .bind(secret_id)
        .bind(&agent)
        .bind(reason)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        self.grant_for(secret_id, &agent).await
    }

    async fn grant_for(&self, secret_id: Uuid, agent: &str) -> CoreResult<SecretGrant> {
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM secret_grants g JOIN secrets s ON s.id = g.secret_id WHERE g.secret_id = ?1 AND g.agent = ?2"
        );
        let grant = sqlx::query_as::<_, SecretGrant>(&sql)
            .bind(secret_id)
            .bind(agent)
            .fetch_one(&self.pool)
            .await?;
        Ok(grant)
    }

    pub async fn list_secret_grants(&self) -> CoreResult<Vec<SecretGrant>> {
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM secret_grants g JOIN secrets s ON s.id = g.secret_id ORDER BY g.status = 'pending' DESC, g.created_at DESC"
        );
        let items = sqlx::query_as::<_, SecretGrant>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(items)
    }

    pub async fn decide_secret_grant(&self, id: Uuid, approve: bool) -> CoreResult<()> {
        let status = if approve {
            GrantStatus::Approved
        } else {
            GrantStatus::Denied
        };
        let result =
            sqlx::query("UPDATE secret_grants SET status = ?1, decided_at = ?2 WHERE id = ?3")
                .bind(status)
                .bind(Utc::now())
                .bind(id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "secret_grant",
                id,
            });
        }
        Ok(())
    }

    pub async fn revoke_secret_grant(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM secret_grants WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "secret_grant",
                id,
            });
        }
        Ok(())
    }

    pub async fn reveal_secret(
        &self,
        vault: &Vault,
        name: &str,
        agent: &str,
    ) -> CoreResult<String> {
        let grant = self.request_secret_access(name, agent, "").await?;
        if grant.status != GrantStatus::Approved {
            return Err(CoreError::SecretAccess {
                secret: grant.secret_name,
                agent: grant.agent,
                status: grant.status.as_str(),
            });
        }
        let (nonce, ciphertext): (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT nonce, ciphertext FROM secrets WHERE id = ?1")
                .bind(grant.secret_id)
                .fetch_one(&self.pool)
                .await?;
        let plaintext = vault.unseal(&grant.secret_name, &nonce, &ciphertext)?;
        sqlx::query("UPDATE secrets SET last_used_at = ?1 WHERE id = ?2")
            .bind(Utc::now())
            .bind(grant.secret_id)
            .execute(&self.pool)
            .await?;
        String::from_utf8(plaintext)
            .map_err(|_| CoreError::Vault("secret is not valid UTF-8".to_string()))
    }
}
