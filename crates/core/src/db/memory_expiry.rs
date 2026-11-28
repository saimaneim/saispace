//! Expiring memories: temporary facts such as "Antigravity has no quota until
//! Friday". While valid they show under their own context section; once past
//! they stop appearing in the context and stop firing triggers.

use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, MemoryContext};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use uuid::Uuid;

/// Parses `--until`: RFC 3339, `YYYY-MM-DD` (end of that day, UTC) or a
/// duration from `now` like `30m`, `6h`, `3d`.
///
/// # Errors
///
/// Returns a validation error for anything else or a time in the past.
pub fn parse_until(raw: &str, now: DateTime<Utc>) -> CoreResult<DateTime<Utc>> {
    let raw = raw.trim();
    let invalid = || {
        CoreError::Validation(format!(
            "fecha de caducidad no válida: {raw} (usa 2026-12-01, 2026-12-01T10:00:00Z, 6h o 3d)"
        ))
    };
    let at = if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        at.with_timezone(&Utc)
    } else if let Ok(day) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        day.and_hms_opt(23, 59, 59).ok_or_else(invalid)?.and_utc()
    } else {
        // Split before the last character, not byte: `3ñ` must not panic.
        let (split, _) = raw.char_indices().last().ok_or_else(invalid)?;
        let (num, unit) = raw.split_at(split);
        let n: i64 = num.parse().map_err(|_| invalid())?;
        let span = match unit {
            "m" => Duration::try_minutes(n),
            "h" => Duration::try_hours(n),
            "d" => Duration::try_days(n),
            _ => None,
        }
        .ok_or_else(invalid)?;
        now.checked_add_signed(span).ok_or_else(invalid)?
    };
    if at <= now {
        return Err(CoreError::Validation(format!("la caducidad {raw} ya pasó")));
    }
    Ok(at)
}

impl Db {
    /// Sets (or with `None` clears) when a memory expires.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory does not exist or the write fails.
    pub async fn set_memory_expiry(
        &self,
        memory_id: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> CoreResult<()> {
        self.get_memory(memory_id).await?;
        match expires_at {
            Some(at) => {
                sqlx::query(
                    "INSERT INTO memory_expiry (memory_id, expires_at) VALUES (?1, ?2)
                     ON CONFLICT(memory_id) DO UPDATE SET expires_at = excluded.expires_at",
                )
                .bind(memory_id)
                .bind(at)
                .execute(&self.pool)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM memory_expiry WHERE memory_id = ?1")
                    .bind(memory_id)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn memory_expiry(&self, memory_id: Uuid) -> CoreResult<Option<DateTime<Utc>>> {
        let row: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT expires_at FROM memory_expiry WHERE memory_id = ?1")
                .bind(memory_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(at,)| at))
    }

    /// Ids of memories already past their expiry at `now`.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn expired_memory_ids(&self, now: DateTime<Utc>) -> CoreResult<Vec<Uuid>> {
        let rows: Vec<(Uuid,)> =
            sqlx::query_as("SELECT memory_id FROM memory_expiry WHERE expires_at <= ?1")
                .bind(now)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Drops expired memories from a session-start context and moves the
    /// still-valid temporary ones into their own section, so each shows once.
    ///
    /// # Errors
    ///
    /// Returns an error if the expiries cannot be loaded.
    pub async fn attach_temporary(
        &self,
        context: &mut MemoryContext,
        now: DateTime<Utc>,
    ) -> CoreResult<()> {
        let rows: Vec<(Uuid, DateTime<Utc>)> =
            sqlx::query_as("SELECT memory_id, expires_at FROM memory_expiry")
                .fetch_all(&self.pool)
                .await?;
        if rows.is_empty() {
            return Ok(());
        }
        let expiry = |m: &Memory| rows.iter().find(|(id, _)| *id == m.id).map(|(_, at)| *at);
        let mut temporary: Vec<(Memory, DateTime<Utc>)> = Vec::new();
        let mut sort = |list: &mut Vec<Memory>| {
            list.retain(|m| match expiry(m) {
                None => true,
                Some(at) if at <= now => false,
                Some(at) => {
                    if !temporary.iter().any(|(t, _)| t.id == m.id) {
                        temporary.push((m.clone(), at));
                    }
                    false
                }
            });
        };
        sort(&mut context.preferences);
        sort(&mut context.environment);
        sort(&mut context.pinned);
        sort(&mut context.memories);
        if let Some(last) = context.last_session.take() {
            let mut one = vec![last];
            sort(&mut one);
            context.last_session = one.pop();
        }
        temporary.sort_by_key(|(_, at)| *at);
        context.temporary = temporary
            .into_iter()
            .map(|(memory, expires_at)| crate::models::TemporaryMemory { memory, expires_at })
            .collect();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_until;
    use chrono::{TimeZone, Utc};

    #[test]
    fn parses_dates_and_durations_and_rejects_the_past() {
        let now = Utc
            .with_ymd_and_hms(2026, 11, 28, 10, 0, 0)
            .single()
            .expect("now");
        assert_eq!(
            parse_until("6h", now).expect("hours"),
            Utc.with_ymd_and_hms(2026, 11, 28, 16, 0, 0)
                .single()
                .expect("t")
        );
        assert_eq!(
            parse_until("2026-12-01", now).expect("day"),
            Utc.with_ymd_and_hms(2026, 12, 1, 23, 59, 59)
                .single()
                .expect("t")
        );
        assert!(parse_until("2026-12-04T21:59:10Z", now).is_ok());
        assert!(parse_until("2026-11-01", now).is_err(), "past");
        assert!(parse_until("soon", now).is_err());
        assert!(parse_until("", now).is_err());
        assert!(parse_until("3ñ", now).is_err(), "multi-byte unit");
        assert!(parse_until("5€", now).is_err());
    }
}
