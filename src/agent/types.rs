use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug)]
pub struct ChatLine {
    pub role: ChatRole,
    pub content: String,
}

impl ChatLine {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolLogLine {
    pub title: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    AssistantText(String),
    AssistantDelta(String),
    ToolStarted {
        id: String,
        name: String,
        summary: String,
    },
    ToolOutput {
        id: String,
        chunk: String,
    },
    ToolFinished {
        id: String,
        output: String,
    },
    ApprovalRequested {
        id: String,
        summary: String,
        detail: String,
    },
    Error(String),
    Done,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActRequest {
    pub action: ToolAction,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    ListFiles,
    ReadFile,
    WriteFile,
    EditFile,
    ApplyPatch,
    Grep,
    RunShell,
    GenerateImageAsset,
    RegenerateImageAsset,
    VaryImageAsset,
    UseAssetAsAppIcon,
    OpenAssetFolder,
    Screenshot,
    MouseClick,
    TypeText,
    Hotkey,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            ok: false,
            output: output.into(),
        }
    }

    pub fn as_model_output(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            "{\"ok\":false,\"output\":\"failed to serialize tool result\"}".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generate_image_asset_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_image_asset","args":{"prompt":"pixel art chest"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::GenerateImageAsset));
    }

    #[test]
    fn parses_use_asset_as_app_icon_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"use_asset_as_app_icon","args":{"source_path":"assets/generated/images/icon.png"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::UseAssetAsAppIcon));
    }

    #[test]
    fn parses_open_asset_folder_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"open_asset_folder","args":{"path":"assets/generated/images"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::OpenAssetFolder));
    }

    #[test]
    fn parses_image_job_followup_actions() {
        let regenerate = serde_json::from_str::<ActRequest>(
            r#"{"action":"regenerate_image_asset","args":{"job_id":"img-1"}}"#,
        )
        .expect("valid regenerate request");
        let variation = serde_json::from_str::<ActRequest>(
            r#"{"action":"vary_image_asset","args":{"job_id":"img-1"}}"#,
        )
        .expect("valid variation request");

        assert!(matches!(
            regenerate.action,
            ToolAction::RegenerateImageAsset
        ));
        assert!(matches!(variation.action, ToolAction::VaryImageAsset));
    }

    #[test]
    fn parses_desktop_control_actions() {
        let click = serde_json::from_str::<ActRequest>(
            r#"{"action":"mouse_click","args":{"x":100,"y":200}}"#,
        )
        .expect("valid click request");
        let typing =
            serde_json::from_str::<ActRequest>(r#"{"action":"type_text","args":{"text":"hello"}}"#)
                .expect("valid type request");
        let hotkey = serde_json::from_str::<ActRequest>(
            r#"{"action":"hotkey","args":{"keys":["ctrl","l"]}}"#,
        )
        .expect("valid hotkey request");

        assert!(matches!(click.action, ToolAction::MouseClick));
        assert!(matches!(typing.action, ToolAction::TypeText));
        assert!(matches!(hotkey.action, ToolAction::Hotkey));
    }
}
