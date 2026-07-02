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
    ProjectCommand,
    GameWorkflow,
    OpenProjectPreview,
    RunSubagent,
    DelegateAgent,
    UpdateWorkspaceContext,
    RecordRunSummary,
    ExportTrace,
    CreateReplayEval,
    OrchestrationSnapshot,
    RunShell,
    GenerateImageAsset,
    GenerateSpritesheetAsset,
    GenerateAudioAsset,
    GenerateVideoAsset,
    RegenerateImageAsset,
    VaryImageAsset,
    UpscaleAsset,
    ExportAsset,
    AttachAsset,
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

    #[test]
    fn parses_project_command_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"project_command","args":{"command":"check","profile":"rust"}}"#,
        )
        .expect("valid project command request");

        assert!(matches!(request.action, ToolAction::ProjectCommand));
    }

    #[test]
    fn parses_game_workflow_and_preview_actions() {
        let workflow = serde_json::from_str::<ActRequest>(
            r#"{"action":"game_workflow","args":{"workflow":"prototype_mechanic","title":"Dash","brief":"fast movement"}}"#,
        )
        .expect("valid workflow request");
        let preview = serde_json::from_str::<ActRequest>(
            r#"{"action":"open_project_preview","args":{"preview":"dev-server"}}"#,
        )
        .expect("valid preview request");

        assert!(matches!(workflow.action, ToolAction::GameWorkflow));
        assert!(matches!(preview.action, ToolAction::OpenProjectPreview));
    }

    #[test]
    fn parses_orchestration_actions() {
        let handoff = serde_json::from_str::<ActRequest>(
            r#"{"action":"delegate_agent","args":{"role":"qa","task":"test combat loop"}}"#,
        )
        .expect("valid handoff request");
        let subagent = serde_json::from_str::<ActRequest>(
            r#"{"action":"run_subagent","args":{"role":"code_agent","task":"inspect parser","max_rounds":3}}"#,
        )
        .expect("valid subagent request");
        let context = serde_json::from_str::<ActRequest>(
            r#"{"action":"update_workspace_context","args":{"summary":"prototype","decisions":["ship demo"]}}"#,
        )
        .expect("valid context request");
        let summary = serde_json::from_str::<ActRequest>(
            r#"{"action":"record_run_summary","args":{"summary":"implemented first pass"}}"#,
        )
        .expect("valid run summary request");
        let trace = serde_json::from_str::<ActRequest>(r#"{"action":"export_trace","args":{}}"#)
            .expect("valid trace request");
        let eval = serde_json::from_str::<ActRequest>(
            r#"{"action":"create_replay_eval","args":{"name":"asset flow","prompt":"make icon"}}"#,
        )
        .expect("valid eval request");
        let snapshot =
            serde_json::from_str::<ActRequest>(r#"{"action":"orchestration_snapshot","args":{}}"#)
                .expect("valid snapshot request");

        assert!(matches!(handoff.action, ToolAction::DelegateAgent));
        assert!(matches!(subagent.action, ToolAction::RunSubagent));
        assert!(matches!(context.action, ToolAction::UpdateWorkspaceContext));
        assert!(matches!(summary.action, ToolAction::RecordRunSummary));
        assert!(matches!(trace.action, ToolAction::ExportTrace));
        assert!(matches!(eval.action, ToolAction::CreateReplayEval));
        assert!(matches!(snapshot.action, ToolAction::OrchestrationSnapshot));
    }

    #[test]
    fn parses_expanded_asset_actions() {
        let spritesheet = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_spritesheet_asset","args":{"prompt":"hero run cycle"}}"#,
        )
        .expect("valid spritesheet request");
        let audio = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_audio_asset","args":{"prompt":"coin pickup sfx"}}"#,
        )
        .expect("valid audio request");
        let video = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_video_asset","args":{"prompt":"game trailer shot"}}"#,
        )
        .expect("valid video request");
        let upscale = serde_json::from_str::<ActRequest>(
            r#"{"action":"upscale_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid upscale request");
        let export = serde_json::from_str::<ActRequest>(
            r#"{"action":"export_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid export request");
        let attach = serde_json::from_str::<ActRequest>(
            r#"{"action":"attach_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid attach request");

        assert!(matches!(
            spritesheet.action,
            ToolAction::GenerateSpritesheetAsset
        ));
        assert!(matches!(audio.action, ToolAction::GenerateAudioAsset));
        assert!(matches!(video.action, ToolAction::GenerateVideoAsset));
        assert!(matches!(upscale.action, ToolAction::UpscaleAsset));
        assert!(matches!(export.action, ToolAction::ExportAsset));
        assert!(matches!(attach.action, ToolAction::AttachAsset));
    }
}
