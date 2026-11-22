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
    fn into_event(self) -> CoreResult<ActivityEvent> {
        let actor = match self.actor_kind.as_str() {
            "agent" => Actor::Agent {
                name: self.actor_name.unwrap_or_default(),
            },
            _ => Actor::User,
        };

        let details = match self.details {
            Some(raw) => Some(serde_json::from_str(&raw)?),
            None => None,
        };

        Ok(ActivityEvent {
            id: self.id,
            project_id: self.project_id,
            actor,
            action: self.action,
            entity: self.entity,
            entity_id: self.entity_id,
            details,
            created_at: self.created_at,
        })
    }
}

impl Db {
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
            r#"
            INSERT INTO activity_events (
                id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            RETURNING id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            "#,
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

        row.into_event()
    }

    pub async fn list_activity(&self, limit: i64) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r#"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event()?);
        }
        Ok(events)
    }

    pub async fn list_activity_by_project(
        &self,
        project_id: Uuid,
        limit: i64,
    ) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r#"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            WHERE project_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event()?);
        }
        Ok(events)
    }

    pub async fn list_activity_by_entity(
        &self,
        entity: EntityKind,
        entity_id: Uuid,
        limit: i64,
    ) -> CoreResult<Vec<ActivityEvent>> {
        let rows = sqlx::query_as::<_, ActivityEventRow>(
            r#"
            SELECT id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at
            FROM activity_events
            WHERE entity = ?1 AND entity_id = ?2
            ORDER BY created_at DESC
            LIMIT ?3
            "#,
        )
        .bind(entity)
        .bind(entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(row.into_event()?);
        }
        Ok(events)
    }
}
