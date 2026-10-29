//! Task acceptance is separate from successful execution. Immutable private
//! receipts carry evidence; neither a model's prose nor a lesson grants authority.
mod checks;
mod command;
mod journal;
pub mod study;
mod study_artifacts;
mod types;

pub(crate) use checks::inspect_inputs_scoped;
pub use checks::{check, inspect_inputs};
pub use command::command;
pub use journal::{History, begin, complete_action, finish, overhead, reserve_action, revise};
pub use types::*;

pub const SPEC: &str = include_str!("../../constitution/task_model.nibli");
pub const PROTOCOL: &str = include_str!("../../../roadmap/S6a.v2.md");
pub const ORIGIN: &str = "lucy:task-model:v1";
pub const POLICY: &str = "bounded-task-lessons-v1";
pub const FORECASTER: &str = "task-window32-beta1-v1";
pub type Result<T> = std::result::Result<T, String>;

pub(crate) fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "timestamp overflow".into())
}
pub(crate) fn identity(prefix: &str) -> Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(format!("{prefix}-{}", crate::store::digest(&bytes)))
}
pub(crate) fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096
}
pub(crate) fn hash(value: &impl serde::Serialize) -> String {
    crate::store::digest(&serde_json::to_vec(value).expect("serializable task data"))
}
pub(crate) fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
