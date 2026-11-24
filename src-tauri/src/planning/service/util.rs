use crate::error::{AppError, AppResult};

pub(super) fn required_text(value: &str, label: &str) -> AppResult<String> {
    non_empty(value.to_string())
        .ok_or_else(|| AppError::Message(format!("{label} cannot be empty")))
}

pub(super) fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(super) fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub(super) fn timestamp() -> String {
    crate::util::time::unix_now_secs().to_string()
}
