use chrono::Utc;
use saispace_core::Db;
use saispace_core::models::RecordActivityInput;
use std::io::Write;
use std::path::Path;

const MAX_LOG_BYTES: u64 = 1024 * 1024;

pub fn rotate_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    if metadata.len() > max_bytes {
        let backup_path = match path.file_name() {
            Some(name) => path.with_file_name(format!("{}.1", name.to_string_lossy())),
            None => std::path::PathBuf::from(format!("{}.1", path.display())),
        };
        std::fs::rename(path, backup_path)?;
    }

    Ok(())
}

pub fn log_error(context: &str, err: &dyn std::fmt::Display) {
    let line = format!("{} [{context}] {err}", Utc::now().to_rfc3339());
    let _ = writeln!(std::io::stderr(), "{line}");
    let Ok(db_path) = saispace_core::default_db_path() else {
        return;
    };
    let Some(dir) = db_path.parent() else {
        return;
    };
    let log_path = dir.join("errors.log");
    let _ = rotate_if_needed(&log_path, MAX_LOG_BYTES);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = writeln!(file, "{line}");
    }
}

pub async fn record_activity_or_log(db: &Db, context: &str, input: RecordActivityInput) {
    if let Err(err) = db.record_activity(input).await {
        log_error(context, &err);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn rotate_missing_file_is_ok() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("errors.log");
        let result = rotate_if_needed(&log_path, 10);
        assert!(result.is_ok());
        assert!(!log_path.exists());
    }

    #[test]
    fn rotate_below_limit_untouched() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("errors.log");
        fs::write(&log_path, "12345").unwrap();
        let result = rotate_if_needed(&log_path, 10);
        assert!(result.is_ok());
        assert!(log_path.exists());
        assert!(!dir.path().join("errors.log.1").exists());
        assert_eq!(fs::read_to_string(&log_path).unwrap(), "12345");
    }

    #[test]
    fn rotate_above_limit_rotates_and_replaces_old_backup() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("errors.log");
        let backup_path = dir.path().join("errors.log.1");

        fs::write(&log_path, "primer contenido largo").unwrap();
        let result = rotate_if_needed(&log_path, 10);
        assert!(result.is_ok());
        assert!(!log_path.exists());
        assert!(backup_path.exists());
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "primer contenido largo"
        );

        fs::write(&log_path, "segundo contenido largo").unwrap();
        let result_second = rotate_if_needed(&log_path, 10);
        assert!(result_second.is_ok());
        assert!(!log_path.exists());
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "segundo contenido largo"
        );
    }
}
