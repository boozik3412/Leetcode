use crate::agent::types::ToolResult;

pub fn screenshot() -> ToolResult {
    ToolResult::error(
        "screenshot is reserved for the desktop-control phase and is not implemented in this MVP",
    )
}
