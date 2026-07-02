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
    Screenshot,
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
