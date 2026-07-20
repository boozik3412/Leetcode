use crate::agent::types::{AppEvent, ToolResult};
use crate::config::{append_journal, AppConfig};
use crate::mcp;
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::mpsc::Sender;

#[derive(Debug, Default, Deserialize)]
pub struct McpSnapshotArgs {}

#[derive(Debug, Deserialize)]
pub struct McpDiscoverArgs {
    pub server: String,
}

#[derive(Debug, Deserialize)]
pub struct McpCallArgs {
    pub server: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
    #[serde(default)]
    pub context_node_id: Option<String>,
}

pub fn snapshot(workspace: &Workspace) -> ToolResult {
    match mcp::registry_json(workspace) {
        Ok(snapshot) => ToolResult::ok(
            serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| snapshot.to_string()),
        ),
        Err(error) => ToolResult::error(error.to_string()),
    }
}

pub async fn discover(
    workspace: &Workspace,
    args: McpDiscoverArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let server = match mcp::server_config(workspace, &args.server) {
        Ok(server) => server,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    let approval_required =
        policy.require_external_approval || server.approval_required_for_discovery();
    if !request_approval_if(
        approval_required,
        events,
        approvals,
        format!("Подключиться к MCP-серверу {}", server.label),
        format!(
            "Транспорт: {}\nЦель: {}\nБудут прочитаны только метаданные и список инструментов. Описания сервера считаются недоверенными.",
            server.transport.label(),
            server.target_label()
        ),
    ) {
        append_journal(format!("mcp\tdiscover_denied\t{}", server.id));
        return ToolResult::error("Подключение к MCP-серверу не подтверждено");
    }

    append_journal(format!("mcp\tdiscover_started\t{}", server.id));
    match mcp::discover_server(workspace, config, &server.id).await {
        Ok(snapshot) => {
            append_journal(format!("mcp\tdiscover_done\t{}", server.id));
            let payload = json!({
                "trust": "untrusted_server_metadata",
                "snapshot": snapshot,
            });
            ToolResult::ok(
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string()),
            )
        }
        Err(error) => {
            append_journal(format!("mcp\tdiscover_error\t{}\t{}", server.id, error));
            ToolResult::error(error.to_string())
        }
    }
}

pub async fn call(
    workspace: &Workspace,
    args: McpCallArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let server = match mcp::server_config(workspace, &args.server) {
        Ok(server) => server,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    if !server.is_tool_allowed(&args.tool) {
        return ToolResult::error(format!(
            "Инструмент {} отсутствует в allowlist сервера {}",
            args.tool, server.id
        ));
    }
    let argument_preview = serde_json::to_string_pretty(&args.arguments)
        .unwrap_or_else(|_| "<не удалось показать JSON>".to_string());
    let project_node_context = crate::project_graph::selected_project_node_context_value(
        workspace,
        args.context_node_id.as_deref(),
    );
    let task_manifest_context =
        crate::game_task_builder::active_task_manifest_context_value(workspace);
    let context_preview = project_node_context
        .as_ref()
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| "узел Project Map не выбран".to_string());
    if !request_approval_if(
        policy.require_external_approval || server.require_approval,
        events,
        approvals,
        format!("Вызвать MCP: {} / {}", server.label, args.tool),
        format!(
            "Сервер: {}\nТранспорт: {}\nИнструмент: {}\nАргументы:\n{}\n\nКонтекст узла Project Map (передаётся через MCP `_meta`):\n{}\n\nРезультат будет обработан как недоверенный ввод.",
            server.target_label(),
            server.transport.label(),
            args.tool,
            compact(&argument_preview, 4_000),
            compact(&context_preview, 4_000)
        ),
    ) {
        append_journal(format!("mcp\tcall_denied\t{}\t{}", server.id, args.tool));
        return ToolResult::error("MCP tool call не подтверждён");
    }

    let argument_bytes = serde_json::to_vec(&args.arguments)
        .map(|value| value.len())
        .unwrap_or_default();
    append_journal(format!(
        "mcp\tcall_started\t{}\t{}\targument_bytes={argument_bytes}",
        server.id, args.tool
    ));
    match mcp::call_server_tool(
        workspace,
        config,
        &server.id,
        &args.tool,
        args.arguments,
        project_node_context,
        task_manifest_context,
    )
    .await
    {
        Ok(output) => {
            append_journal(format!(
                "mcp\tcall_done\t{}\t{}\toutput_chars={}",
                server.id,
                args.tool,
                output.chars().count()
            ));
            ToolResult::ok(output)
        }
        Err(error) => {
            append_journal(format!(
                "mcp\tcall_error\t{}\t{}\t{}",
                server.id, args.tool, error
            ));
            ToolResult::error(error.to_string())
        }
    }
}

fn compact(value: &str, max_chars: usize) -> String {
    let mut compact = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        compact.push_str("\n... [обрезано]");
    }
    compact
}
