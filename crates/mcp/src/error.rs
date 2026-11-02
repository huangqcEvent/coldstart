use coldstart_app::AppError;
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

pub(crate) fn app_error_result(error: AppError) -> CallToolResult {
    let details = error
        .review_details()
        .or_else(|| error.verification_details());
    tool_error_result(error.code(), error.to_string(), details)
}

pub(crate) fn tool_error_result(
    code: impl Into<String>,
    message: impl Into<String>,
    details: Option<Value>,
) -> CallToolResult {
    let mut payload = json!({
        "code": code.into(),
        "message": message.into(),
    });
    if let Some(details) = details {
        payload["details"] = details;
    }
    CallToolResult::structured_error(payload)
}

pub(crate) fn invalid_id_result(field: &str, value: &str) -> CallToolResult {
    tool_error_result(
        "invalid_id",
        format!("{field} must be a valid UUID"),
        Some(json!({ "field": field, "value": value })),
    )
}

pub(crate) fn invalid_capability_id_result(value: &str) -> CallToolResult {
    tool_error_result(
        "invalid_id",
        "capability_id must be a valid blueprint capability ID",
        Some(json!({ "field": "capability_id", "value": value })),
    )
}
