use chrono::Utc;
use saispace_core::Db;
use saispace_core::models::RecordActivityInput;
use std::io::Write;

pub fn log_error(context: &str, err: &dyn std::fmt::Display) {
    let line = format!("{} [{context}] {err}", Utc::now().to_rfc3339());
    let _ = writeln!(std::io::stderr(), "{line}");
    let Ok(db_path) = saispace_core::default_db_path() else {
        return;
    };
    let Some(dir) = db_path.parent() else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("errors.log"))
    {
        let _ = writeln!(file, "{line}");
    }
}

pub async fn record_activity_or_log(db: &Db, context: &str, input: RecordActivityInput) {
    if let Err(err) = db.record_activity(input).await {
        log_error(context, &err);
    }
}
