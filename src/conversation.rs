use crate::agent::types::{ChatLine, ChatRole};
use crate::agent::AgentState;
use crate::agent_history::load_agent_history_tail;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONVERSATION_DIR: &str = "assets/generated/leetcode/conversations";
pub const CONVERSATION_INDEX_PATH: &str = "assets/generated/leetcode/conversations/index.json";
pub const CONVERSATION_STATE_PATH: &str = "assets/generated/leetcode/conversation_state.json";

const MAX_TRANSCRIPT_BYTES: usize = 8_000_000;
const RECENT_MESSAGE_LIMIT: usize = 14;
const RELEVANT_MESSAGE_LIMIT: usize = 8;
const ROLLING_SUMMARY_CHARS: usize = 4_000;
const CONTEXT_MESSAGE_CHARS: usize = 900;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ConversationIndex {
    #[serde(default)]
    pub active_id: Option<String>,
    #[serde(default)]
    pub conversations: Vec<ConversationMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ConversationRuntimeState {
    #[serde(default)]
    pub active_conversation_id: Option<String>,
    #[serde(default)]
    pub rolling_summary: String,
    #[serde(default)]
    pub agent_state: Option<AgentState>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug)]
pub struct LoadedConversation {
    pub id: String,
    pub index: ConversationIndex,
    pub state: ConversationRuntimeState,
    pub chat: Vec<ChatLine>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AgentContextSnapshot {
    pub conversation_id: String,
    pub rolling_summary: String,
    pub recent_messages: Vec<ContextMessage>,
    pub relevant_messages: Vec<ContextMessage>,
    pub recent_runs: Vec<String>,
}

impl AgentContextSnapshot {
    pub fn is_empty(&self) -> bool {
        self.rolling_summary.trim().is_empty()
            && self.recent_messages.is_empty()
            && self.relevant_messages.is_empty()
            && self.recent_runs.is_empty()
    }

    pub fn to_prompt_block(&self) -> String {
        if self.is_empty() {
            return "Контекст переписки: сохранённой истории пока нет.".to_string();
        }

        let rolling = if self.rolling_summary.trim().is_empty() {
            "нет".to_string()
        } else {
            self.rolling_summary.clone()
        };
        let recent = format_messages(&self.recent_messages);
        let relevant = format_messages(&self.relevant_messages);
        let runs = if self.recent_runs.is_empty() {
            "нет".to_string()
        } else {
            self.recent_runs
                .iter()
                .map(|run| format!("- {run}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            "Контекст переписки Leetcode:\nconversation_id: {}\n\nСжатая старая история:\n{}\n\nПоследние сообщения:\n{}\n\nРелевантные старые сообщения:\n{}\n\nПоследние сохранённые запуски:\n{}\n\nИспользуй этот блок как вспомогательную память. Если он конфликтует с текущим запросом пользователя, текущий запрос важнее.",
            self.conversation_id,
            rolling,
            empty_label(&recent),
            empty_label(&relevant),
            runs
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
}

pub fn default_chat() -> Vec<ChatLine> {
    vec![ChatLine::system(
        "Выберите проект, проверьте модель/API-ключ и отправьте задачу агенту.",
    )]
}

pub fn load_active_conversation(workspace: &Workspace) -> LoadedConversation {
    let mut index = load_index(workspace);
    let mut state = load_state(workspace);
    let now = unix_timestamp();
    let active_id = state
        .active_conversation_id
        .clone()
        .or_else(|| index.active_id.clone())
        .filter(|id| index.conversations.iter().any(|meta| meta.id == *id))
        .unwrap_or_else(|| {
            let id = new_conversation_id();
            index.active_id = Some(id.clone());
            state.active_conversation_id = Some(id.clone());
            index.conversations.push(ConversationMeta {
                id: id.clone(),
                title: "Новый чат".to_string(),
                created_at: now,
                updated_at: now,
                message_count: 0,
                pinned: false,
            });
            id
        });

    index.active_id = Some(active_id.clone());
    state.active_conversation_id = Some(active_id.clone());
    let mut state = load_state_for_conversation(workspace, &active_id).unwrap_or(state);
    let chat = load_transcript(workspace, &active_id);
    let chat = if chat.is_empty() {
        default_chat()
    } else {
        chat
    };
    refresh_index_meta(&mut index, &active_id, &chat);
    state.rolling_summary = build_rolling_summary(&chat);
    state.updated_at = now;
    let _ = save_index(workspace, &index);
    let _ = save_state(workspace, &state);
    let _ = save_state_for_conversation(workspace, &active_id, &state);
    let _ = save_transcript(workspace, &active_id, &chat);

    LoadedConversation {
        id: active_id,
        index,
        state,
        chat,
        status: "переписка восстановлена".to_string(),
    }
}

pub fn create_new_conversation(workspace: &Workspace) -> anyhow::Result<LoadedConversation> {
    let mut index = load_index(workspace);
    let mut state = load_state(workspace);
    let now = unix_timestamp();
    let id = new_conversation_id();
    let chat = vec![ChatLine::system(
        "Новый чат создан. Рабочая папка и настройки сохранены.",
    )];
    index.active_id = Some(id.clone());
    state.active_conversation_id = Some(id.clone());
    state.rolling_summary.clear();
    state.agent_state = None;
    state.updated_at = now;
    index.conversations.push(ConversationMeta {
        id: id.clone(),
        title: "Новый чат".to_string(),
        created_at: now,
        updated_at: now,
        message_count: chat.len(),
        pinned: false,
    });
    save_transcript(workspace, &id, &chat)?;
    save_index(workspace, &index)?;
    save_state(workspace, &state)?;
    save_state_for_conversation(workspace, &id, &state)?;

    Ok(LoadedConversation {
        id,
        index,
        state,
        chat,
        status: "создан новый чат".to_string(),
    })
}

pub fn save_conversation_snapshot(
    workspace: &Workspace,
    conversation_id: &str,
    chat: &[ChatLine],
    agent_state: Option<AgentState>,
) -> anyhow::Result<ConversationRuntimeState> {
    let mut index = load_index(workspace);
    let now = unix_timestamp();
    if !index
        .conversations
        .iter()
        .any(|meta| meta.id == conversation_id)
    {
        index.conversations.push(ConversationMeta {
            id: conversation_id.to_string(),
            title: conversation_title(chat),
            created_at: now,
            updated_at: now,
            message_count: chat.len(),
            pinned: false,
        });
    }
    index.active_id = Some(conversation_id.to_string());
    refresh_index_meta(&mut index, conversation_id, chat);
    save_transcript(workspace, conversation_id, chat)?;
    save_index(workspace, &index)?;

    let mut state = load_state(workspace);
    state.active_conversation_id = Some(conversation_id.to_string());
    state.rolling_summary = build_rolling_summary(chat);
    state.agent_state = agent_state;
    state.updated_at = now;
    save_state(workspace, &state)?;
    save_state_for_conversation(workspace, conversation_id, &state)?;
    Ok(state)
}

pub fn compile_context_snapshot(
    workspace: &Workspace,
    conversation_id: &str,
    chat: &[ChatLine],
    query: &str,
) -> AgentContextSnapshot {
    let rolling_summary = build_rolling_summary(chat);
    let recent_start = chat.len().saturating_sub(RECENT_MESSAGE_LIMIT);
    let recent_messages = chat[recent_start..]
        .iter()
        .filter(|line| !line.content.trim().is_empty())
        .map(context_message_from_line)
        .collect::<Vec<_>>();
    let recent_ids = recent_start..chat.len();
    let recent_ids = recent_ids.collect::<HashSet<_>>();
    let query_terms = query_terms(query);
    let mut scored = chat
        .iter()
        .enumerate()
        .filter(|(index, line)| !recent_ids.contains(index) && !line.content.trim().is_empty())
        .map(|(_, line)| (score_message(&line.content, &query_terms), line))
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let relevant_messages = scored
        .into_iter()
        .take(RELEVANT_MESSAGE_LIMIT)
        .map(|(_, line)| context_message_from_line(line))
        .collect::<Vec<_>>();
    let recent_runs = load_agent_history_tail(workspace, 5)
        .into_iter()
        .rev()
        .map(|record| {
            format!(
                "{} · {} · файлы: {} · {}",
                record.status,
                record.model,
                record.changed_files.len(),
                compact_inline(&record.user_request, 220)
            )
        })
        .collect::<Vec<_>>();

    AgentContextSnapshot {
        conversation_id: conversation_id.to_string(),
        rolling_summary,
        recent_messages,
        relevant_messages,
        recent_runs,
    }
}

pub fn load_index(workspace: &Workspace) -> ConversationIndex {
    workspace
        .read_text(CONVERSATION_INDEX_PATH, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_index(workspace: &Workspace, index: &ConversationIndex) -> anyhow::Result<()> {
    workspace.write_text(
        CONVERSATION_INDEX_PATH,
        &serde_json::to_string_pretty(index)?,
    )
}

pub fn load_state(workspace: &Workspace) -> ConversationRuntimeState {
    workspace
        .read_text(CONVERSATION_STATE_PATH, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_state(workspace: &Workspace, state: &ConversationRuntimeState) -> anyhow::Result<()> {
    workspace.write_text(
        CONVERSATION_STATE_PATH,
        &serde_json::to_string_pretty(state)?,
    )
}

fn transcript_path(id: &str) -> String {
    format!("{CONVERSATION_DIR}/{id}.jsonl")
}

fn conversation_state_path(id: &str) -> String {
    format!("{CONVERSATION_DIR}/{id}.state.json")
}

fn load_state_for_conversation(
    workspace: &Workspace,
    id: &str,
) -> Option<ConversationRuntimeState> {
    workspace
        .read_text(&conversation_state_path(id), 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn save_state_for_conversation(
    workspace: &Workspace,
    id: &str,
    state: &ConversationRuntimeState,
) -> anyhow::Result<()> {
    workspace.write_text(
        &conversation_state_path(id),
        &serde_json::to_string_pretty(state)?,
    )
}

fn load_transcript(workspace: &Workspace, id: &str) -> Vec<ChatLine> {
    workspace
        .read_text(&transcript_path(id), MAX_TRANSCRIPT_BYTES)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<ChatLine>(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn save_transcript(workspace: &Workspace, id: &str, chat: &[ChatLine]) -> anyhow::Result<()> {
    let path = workspace.resolve_for_write(&transcript_path(id))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = chat
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

fn refresh_index_meta(index: &mut ConversationIndex, id: &str, chat: &[ChatLine]) {
    let now = unix_timestamp();
    let title = conversation_title(chat);
    if let Some(meta) = index.conversations.iter_mut().find(|meta| meta.id == id) {
        meta.title = title;
        meta.updated_at = now;
        meta.message_count = chat.len();
    }
    index.conversations.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.updated_at.cmp(&a.updated_at))
    });
}

fn conversation_title(chat: &[ChatLine]) -> String {
    chat.iter()
        .find(|line| matches!(line.role, ChatRole::User) && !line.content.trim().is_empty())
        .map(|line| compact_inline(&line.content, 48))
        .unwrap_or_else(|| "Новый чат".to_string())
}

fn build_rolling_summary(chat: &[ChatLine]) -> String {
    let older_count = chat.len().saturating_sub(RECENT_MESSAGE_LIMIT);
    if older_count == 0 {
        return String::new();
    }
    let mut lines = Vec::new();
    for line in chat.iter().take(older_count).rev().take(40).rev() {
        if line.content.trim().is_empty() {
            continue;
        }
        lines.push(format!(
            "- {}: {}",
            role_label(&line.role),
            compact_inline(&line.content, 260)
        ));
    }
    compact_inline(&lines.join("\n"), ROLLING_SUMMARY_CHARS)
}

fn context_message_from_line(line: &ChatLine) -> ContextMessage {
    ContextMessage {
        role: role_label(&line.role).to_string(),
        content: compact_inline(&line.content, CONTEXT_MESSAGE_CHARS),
    }
}

fn format_messages(messages: &[ContextMessage]) -> String {
    messages
        .iter()
        .map(|message| format!("- {}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn role_label(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "system",
    }
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .map(str::trim)
        .filter(|word| word.chars().count() >= 4)
        .map(ToString::to_string)
        .collect()
}

fn score_message(content: &str, terms: &[String]) -> usize {
    let lower = content.to_lowercase();
    terms.iter().filter(|term| lower.contains(*term)).count()
}

fn new_conversation_id() -> String {
    format!(
        "chat-{}-{}",
        unix_timestamp(),
        uuid::Uuid::new_v4().simple()
    )
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn compact_inline(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        format!(
            "{}...",
            normalized.chars().take(max_chars).collect::<String>()
        )
    }
}

fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "нет".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_restores_active_conversation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let loaded = create_new_conversation(&workspace).expect("create");
        let chat = vec![
            ChatLine::system("Система"),
            ChatLine::user("Нужно запомнить архитектуру проекта"),
            ChatLine::assistant("Запомнил архитектуру."),
        ];

        let saved_state = save_conversation_snapshot(
            &workspace,
            &loaded.id,
            &chat,
            Some(AgentState {
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-5.5".to_string()),
                previous_response_id: Some("resp_123".to_string()),
                provider_state: None,
            }),
        )
        .expect("save");

        let restored = load_active_conversation(&workspace);
        assert_eq!(restored.id, loaded.id);
        assert_eq!(restored.chat.len(), 3);
        assert_eq!(
            saved_state
                .agent_state
                .unwrap()
                .previous_response_id
                .as_deref(),
            Some("resp_123")
        );
        assert_eq!(
            restored
                .state
                .agent_state
                .unwrap()
                .previous_response_id
                .as_deref(),
            Some("resp_123")
        );
    }

    #[test]
    fn context_snapshot_retrieves_relevant_old_messages() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let mut chat = vec![
            ChatLine::system("Система"),
            ChatLine::user("В проекте важна память контекста и transcript store"),
        ];
        for index in 0..20 {
            chat.push(ChatLine::assistant(format!("Обычное сообщение {index}")));
        }

        let snapshot =
            compile_context_snapshot(&workspace, "chat-test", &chat, "как работает transcript");

        assert!(snapshot
            .relevant_messages
            .iter()
            .any(|message| message.content.contains("transcript store")));
        assert!(!snapshot.rolling_summary.is_empty());
    }
}
