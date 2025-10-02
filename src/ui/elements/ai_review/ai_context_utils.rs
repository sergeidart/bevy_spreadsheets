// Moved from editor/ai_context_utils.rs to ai_review/ai_context_utils.rs
use crate::sheets::definitions::ColumnDataType;

pub fn decorate_context_with_type(
    context: Option<&String>,
    data_type: ColumnDataType,
) -> Option<String> {
    let ctx_ref = context?;
    let trimmed = ctx_ref.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let type_label = match data_type {
        ColumnDataType::String => Some("String"),
        ColumnDataType::Bool => Some("Bool"),
        ColumnDataType::I64 => Some("Integer"),
        ColumnDataType::F64 => Some("Float"),
    };

    if let Some(label) = type_label {
        let mut result = trimmed.to_string();
        // Use raw string to avoid escape interpretation
        result.push_str(" \\ ");
        result.push_str(label);
        Some(result)
    } else {
        Some(ctx_ref.clone())
    }
}
