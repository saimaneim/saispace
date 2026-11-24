use crate::error::{CoreError, CoreResult};

pub const MAX_NAME_CHARS: usize = 200;

pub fn required_text(field: &str, value: &str) -> CoreResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Validation(format!("{field} must not be empty")));
    }
    Ok(trimmed.to_string())
}

pub fn limited_text(field: &str, value: &str) -> CoreResult<String> {
    let trimmed = required_text(field, value)?;
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(CoreError::Validation(format!(
            "{field} must be at most {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(trimmed)
}

pub fn text_with_limit(field: &str, value: &str, max_chars: usize) -> CoreResult<String> {
    let trimmed = required_text(field, value)?;
    if trimmed.chars().count() > max_chars {
        return Err(CoreError::Validation(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    Ok(trimmed)
}
