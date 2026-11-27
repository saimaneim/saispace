use crate::error::CommandResult;
use saispace_core::models::KpiHistory;
use saispace_core::{CoreError, Db};
use tauri::State;
use uuid::Uuid;

fn normalize_kpi_scope(scope: Option<&str>) -> CommandResult<String> {
    let Some(raw) = scope.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok("global".to_string());
    };
    if raw == "global" {
        return Ok("global".to_string());
    }
    let id = Uuid::parse_str(raw)
        .map_err(|err| CoreError::Validation(format!("scope de KPI inválido '{raw}': {err}")))?;
    Ok(id.to_string())
}

/// # Errors
///
/// Returns an error if the KPI history cannot be loaded.
#[tauri::command]
pub async fn kpi_history(
    days: Option<i64>,
    scope: Option<String>,
    db: State<'_, Db>,
) -> CommandResult<KpiHistory> {
    let scope = normalize_kpi_scope(scope.as_deref())?;
    let days = days.unwrap_or(30);
    Ok(db.kpi_history(&scope, days).await?)
}

#[cfg(test)]
mod tests {
    use super::normalize_kpi_scope;
    use uuid::Uuid;

    #[test]
    fn normalize_kpi_scope_handles_global_uuids_and_rejects_names() {
        assert_eq!(normalize_kpi_scope(None).expect("none"), "global");
        assert_eq!(normalize_kpi_scope(Some("  ")).expect("empty"), "global");
        assert_eq!(
            normalize_kpi_scope(Some("global")).expect("global"),
            "global"
        );

        let id = Uuid::now_v7();
        let upper = id.to_string().to_uppercase();
        assert_eq!(
            normalize_kpi_scope(Some(&upper)).expect("uuid"),
            id.to_string()
        );
        assert!(normalize_kpi_scope(Some("alpha")).is_err());
    }
}
