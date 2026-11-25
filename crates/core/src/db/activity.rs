use crate::db::Db;
use crate::error::CoreResult;
use crate::models::{ActionKind, ActivityEvent, Actor, EntityKind, RecordActivityInput};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct ActivityEventRow {
    id: Uuid,
    project_id: Option<Uuid>,
    actor_kind: String,
    actor_name: Option<String>,
    action: ActionKind,
    entity: EntityKind,
    entity_id: Option<Uuid>,
    details: Option<String>,
    created_at: DateTime<Utc>,
}

impl ActivityEventRow {
    fn into_event(self) -> ActivityEvent {
        let actor = match self.actor_kind.as_str() {
            "agent" => Actor::Agent {
                name: self.actor_name.unwrap_or_default(),
            },
            _ => Actor::User,
        };

        let details = match self.details {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match serde_json::from_str::<serde_json::Value>(trimmed) {
                        Ok(val) => Some(val),
                        Err(_) => Some(serde_json::Value::String(trimmed.to_string())),
                    }
                }
            }
            None => None,
        };

        ActivityEvent {
            id: self.id,
            project_id: self.project_id,
            actor,
            action: self.action,
            entity: self.entity,
            entity_id: self.entity_id,
            details,
            created_at: self.created_at,
        }
    }
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the activity row cannot be inserted.
    pub async fn record_activity(&self, input: RecordActivityInput) -> CoreResult<ActivityEvent> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let (actor_kind, actor_name) = match &input.actor {
            Actor::User => ("user", None),
            Actor::Agent { name } => ("agent", Some(name.as_str())),
        };
        let details_str = match &input.details {
            Some(details) => Some(serde_json::to_string(details)?),
            None => None,
        };

        let row = sqlx::query_as::<_, ActivityEventRow>(
            r"
            INSERT INTO activity_events (
                id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            RETURNING id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            ",
        )
        .bind(id)
        .bind(input.project_id)
        .bind(actor_kind)
        .bind(actor_name)
        .bind(input.action)
        .bind(input.entity)
        .bind(input.entity_id)
        .bind(details_str)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_event())
    }

    /// # Errors
    ///
    /// Returns an error if the activity rows cannot be loaded.
    pub async fn list_activity(&self, limit: i64) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            ORDER BY created_at DESC
            LIMIT ?1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event());
        }
        Ok(events)
    }

    /// # Errors
    ///
    /// Returns an error if the activity rows cannot be loaded.
    pub async fn list_activity_by_project(
        &self,
        project_id: Uuid,
        limit: i64,
    ) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            WHERE project_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            ",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event());
        }
        Ok(events)
    }

    /// # Errors
    ///
    /// Returns an error if the activity rows cannot be loaded.
    pub async fn list_activity_by_entity(
        &self,
        entity: EntityKind,
        entity_id: Uuid,
        limit: i64,
    ) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            WHERE entity = ?1 AND entity_id = ?2
            ORDER BY created_at DESC
            LIMIT ?3
            ",
        )
        .bind(entity)
        .bind(entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event());
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_row(details: Option<String>) -> ActivityEventRow {
        ActivityEventRow {
            id: Uuid::now_v7(),
            project_id: None,
            actor_kind: "user".to_string(),
            actor_name: None,
            action: ActionKind::Create,
            entity: EntityKind::Project,
            entity_id: None,
            details,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn parses_valid_json_details() {
        let row = create_test_row(Some(r#"{"action":"created","version":1}"#.to_string()));
        let event = row.into_event();
        assert_eq!(
            event.details,
            Some(serde_json::json!({"action": "created", "version": 1}))
        );
    }

    #[test]
    fn parses_plain_text_details_into_json_string() {
        let row = create_test_row(Some("Registró proyecto saispace".to_string()));
        let event = row.into_event();
        assert_eq!(
            event.details,
            Some(serde_json::Value::String(
                "Registró proyecto saispace".to_string()
            ))
        );
    }

    #[test]
    fn treats_empty_string_details_as_none() {
        let row = create_test_row(Some(String::new()));
        let event = row.into_event();
        assert_eq!(event.details, None);
    }

    #[test]
    fn treats_whitespace_string_details_as_none() {
        let row = create_test_row(Some("   \n\t  ".to_string()));
        let event = row.into_event();
        assert_eq!(event.details, None);
    }

    #[test]
    fn treats_none_details_as_none() {
        let row = create_test_row(None);
        let event = row.into_event();
        assert_eq!(event.details, None);
    }
}
