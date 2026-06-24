pub mod context;
pub mod graph;
pub mod indexing;
pub mod navigate;
pub mod search;

/// Validate that a required string argument is not empty/whitespace-only.
pub(crate) fn require_non_empty(value: &str, name: &str) -> Result<(), rmcp::ErrorData> {
    if value.trim().is_empty() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("'{name}' must not be empty"),
            None,
        ));
    }
    Ok(())
}
