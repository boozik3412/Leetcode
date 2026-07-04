use crate::agent::types::{ChatLine, ChatRole};
use crate::agent::AgentState;
use crate::agent_history::load_agent_history_tail;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONVERSATION_DIR: &str = "assets/generated/leetcode/conversations";
pub const CONVERSATION_INDEX_PATH: &str = "assets/generated/leetcode/conversations/index.json";
pub const CONVERSATION_STATE_PATH: &str = "assets/generated/leetcode/conversation_state.json";
pub const CONTEXT_PROFILE_DIR: &str = "assets/generated/leetcode/context_profiles";

const MAX_TRANSCRIPT_BYTES: usize = 8_000_000;
const MAX_CONTEXT_PROFILE_BYTES: u64 = 1_000_000;
const DEFAULT_RECENT_MESSAGE_LIMIT: usize = 14;
const DEFAULT_RELEVANT_MESSAGE_LIMIT: usize = 8;
const DEFAULT_RECENT_RUN_LIMIT: usize = 5;
const ROLLING_SUMMARY_CHARS: usize = 4_000;
const CONTEXT_MESSAGE_CHARS: usize = 900;
const CONTEXT_NOTE_LIMIT: usize = 20;
const CONTEXT_NOTE_CHARS: usize = 500;

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
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub custom_title: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ConversationRuntimeState {
    #[serde(default)]
    pub active_conversation_id: Option<String>,
    #[serde(default)]
    pub rolling_summary: String,
    #[serde(default)]
    pub context_notes: Vec<String>,
    #[serde(default)]
    pub agent_state: Option<AgentState>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextProfile {
    pub schema_version: u32,
    pub exported_at: u64,
    pub source_conversation_id: String,
    pub title: String,
    pub context_notes: Vec<String>,
    pub budget: ContextBudget,
}

#[derive(Clone, Debug)]
pub struct ContextProfileEntry {
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub profile: ContextProfile,
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
    #[serde(default)]
    pub pinned_notes: Vec<String>,
    pub recent_messages: Vec<ContextMessage>,
    pub relevant_messages: Vec<ContextMessage>,
    pub recent_runs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ContextBudget {
    pub recent_message_limit: usize,
    pub relevant_message_limit: usize,
    pub recent_run_limit: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            recent_message_limit: DEFAULT_RECENT_MESSAGE_LIMIT,
            relevant_message_limit: DEFAULT_RELEVANT_MESSAGE_LIMIT,
            recent_run_limit: DEFAULT_RECENT_RUN_LIMIT,
        }
    }
}

impl ContextBudget {
    pub fn bounded(self) -> Self {
        Self {
            recent_message_limit: self.recent_message_limit.min(80),
            relevant_message_limit: self.relevant_message_limit.min(40),
            recent_run_limit: self.recent_run_limit.min(20),
        }
    }
}

impl AgentContextSnapshot {
    pub fn is_empty(&self) -> bool {
        self.rolling_summary.trim().is_empty()
            && self.pinned_notes.is_empty()
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
        let pinned_notes = if self.pinned_notes.is_empty() {
            "нет".to_string()
        } else {
            self.pinned_notes
                .iter()
                .map(|note| format!("- {note}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
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
            "Контекст переписки Leetcode:\nconversation_id: {}\n\nЗакреплённые заметки чата:\n{}\n\nСжатая старая история:\n{}\n\nПоследние сообщения:\n{}\n\nРелевантные старые сообщения:\n{}\n\nПоследние сохранённые запуски:\n{}\n\nИспользуй этот блок как вспомогательную память. Если он конфликтует с текущим запросом пользователя, текущий запрос важнее.",
            self.conversation_id,
            pinned_notes,
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
        .filter(|id| {
            index
                .conversations
                .iter()
                .any(|meta| meta.id == *id && !meta.archived)
        })
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
                archived: false,
                custom_title: false,
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
    state.context_notes = normalize_context_notes(state.context_notes);
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
        archived: false,
        custom_title: false,
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
            archived: false,
            custom_title: false,
        });
    }
    index.active_id = Some(conversation_id.to_string());
    refresh_index_meta(&mut index, conversation_id, chat);
    save_transcript(workspace, conversation_id, chat)?;
    save_index(workspace, &index)?;

    let mut state = load_state_for_conversation(workspace, conversation_id)
        .unwrap_or_else(|| load_state(workspace));
    state.active_conversation_id = Some(conversation_id.to_string());
    state.rolling_summary = build_rolling_summary(chat);
    state.context_notes = normalize_context_notes(state.context_notes);
    state.agent_state = agent_state;
    state.updated_at = now;
    save_state(workspace, &state)?;
    save_state_for_conversation(workspace, conversation_id, &state)?;
    Ok(state)
}

pub fn rename_conversation(
    workspace: &Workspace,
    conversation_id: &str,
    title: &str,
) -> anyhow::Result<ConversationIndex> {
    let title = compact_inline(title.trim(), 80);
    if title.is_empty() {
        anyhow::bail!("Название чата не может быть пустым");
    }

    let mut index = load_index(workspace);
    let Some(meta) = index
        .conversations
        .iter_mut()
        .find(|meta| meta.id == conversation_id)
    else {
        anyhow::bail!("Чат не найден: {conversation_id}");
    };
    meta.title = title;
    meta.custom_title = true;
    meta.updated_at = unix_timestamp();
    sort_index(&mut index);
    save_index(workspace, &index)?;
    Ok(index)
}

pub fn set_conversation_pinned(
    workspace: &Workspace,
    conversation_id: &str,
    pinned: bool,
) -> anyhow::Result<ConversationIndex> {
    let mut index = load_index(workspace);
    let Some(meta) = index
        .conversations
        .iter_mut()
        .find(|meta| meta.id == conversation_id)
    else {
        anyhow::bail!("Чат не найден: {conversation_id}");
    };
    meta.pinned = pinned;
    meta.updated_at = unix_timestamp();
    sort_index(&mut index);
    save_index(workspace, &index)?;
    Ok(index)
}

pub fn archive_conversation(
    workspace: &Workspace,
    conversation_id: &str,
) -> anyhow::Result<LoadedConversation> {
    let mut index = load_index(workspace);
    let Some(meta) = index
        .conversations
        .iter_mut()
        .find(|meta| meta.id == conversation_id)
    else {
        anyhow::bail!("Чат не найден: {conversation_id}");
    };
    meta.archived = true;
    meta.pinned = false;
    meta.updated_at = unix_timestamp();
    if index.active_id.as_deref() == Some(conversation_id) {
        index.active_id = next_available_conversation_id(&index);
    }
    sort_index(&mut index);
    save_index(workspace, &index)?;
    sync_active_state(workspace, index.active_id.clone())?;
    Ok(load_active_conversation(workspace))
}

pub fn restore_conversation(
    workspace: &Workspace,
    conversation_id: &str,
) -> anyhow::Result<LoadedConversation> {
    let mut index = load_index(workspace);
    let Some(meta) = index
        .conversations
        .iter_mut()
        .find(|meta| meta.id == conversation_id)
    else {
        anyhow::bail!("Чат не найден: {conversation_id}");
    };
    meta.archived = false;
    meta.updated_at = unix_timestamp();
    index.active_id = Some(conversation_id.to_string());
    sort_index(&mut index);
    save_index(workspace, &index)?;
    sync_active_state(workspace, Some(conversation_id.to_string()))?;
    Ok(load_active_conversation(workspace))
}

pub fn delete_conversation(
    workspace: &Workspace,
    conversation_id: &str,
) -> anyhow::Result<LoadedConversation> {
    let mut index = load_index(workspace);
    let old_len = index.conversations.len();
    index
        .conversations
        .retain(|meta| meta.id != conversation_id);
    if index.conversations.len() == old_len {
        anyhow::bail!("Чат не найден: {conversation_id}");
    }
    if index.active_id.as_deref() == Some(conversation_id) {
        index.active_id = next_available_conversation_id(&index);
    }
    sort_index(&mut index);
    save_index(workspace, &index)?;
    sync_active_state(workspace, index.active_id.clone())?;
    remove_conversation_files(workspace, conversation_id)?;
    Ok(load_active_conversation(workspace))
}

pub fn save_conversation_context_notes(
    workspace: &Workspace,
    conversation_id: &str,
    notes: Vec<String>,
) -> anyhow::Result<ConversationRuntimeState> {
    let mut state = load_state_for_conversation(workspace, conversation_id)
        .unwrap_or_else(|| load_state(workspace));
    state.active_conversation_id = Some(conversation_id.to_string());
    state.context_notes = normalize_context_notes(notes);
    state.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    save_state_for_conversation(workspace, conversation_id, &state)?;
    Ok(state)
}

pub fn export_context_profile(
    workspace: &Workspace,
    conversation_id: &str,
    title: &str,
    notes: &[String],
    budget: ContextBudget,
) -> anyhow::Result<String> {
    let exported_at = unix_timestamp();
    let profile = ContextProfile {
        schema_version: 1,
        exported_at,
        source_conversation_id: conversation_id.to_string(),
        title: compact_inline(title.trim(), 120),
        context_notes: normalize_context_notes(notes.to_vec()),
        budget: budget.bounded(),
    };
    let rel_path = format!(
        "{CONTEXT_PROFILE_DIR}/context-profile-{exported_at}-{}.json",
        safe_identifier(conversation_id)
    );
    workspace.write_text(&rel_path, &serde_json::to_string_pretty(&profile)?)?;
    Ok(rel_path)
}

pub fn import_context_profile_file(
    workspace: &Workspace,
    path: &Path,
    conversation_id: &str,
) -> anyhow::Result<(ConversationRuntimeState, ContextBudget)> {
    let profile = read_context_profile_file(path)?;
    let budget = profile.budget.bounded();
    let state = save_conversation_context_notes(
        workspace,
        conversation_id,
        normalize_context_notes(profile.context_notes),
    )?;
    Ok((state, budget))
}

pub fn read_context_profile_file(path: &Path) -> anyhow::Result<ContextProfile> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_CONTEXT_PROFILE_BYTES {
        anyhow::bail!(
            "Context profile is too large: {} bytes, limit is {} bytes",
            metadata.len(),
            MAX_CONTEXT_PROFILE_BYTES
        );
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn list_context_profiles(workspace: &Workspace) -> Vec<ContextProfileEntry> {
    let Ok(dir) = workspace.resolve_for_write(CONTEXT_PROFILE_DIR) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut profiles = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .filter_map(|entry| {
            let abs_path = entry.path();
            let profile = read_context_profile_file(&abs_path).ok()?;
            let rel_path = abs_path
                .strip_prefix(workspace.root())
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some(ContextProfileEntry {
                rel_path,
                abs_path,
                profile,
            })
        })
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| right.profile.exported_at.cmp(&left.profile.exported_at));
    profiles
}

#[cfg(test)]
pub fn compile_context_snapshot(
    workspace: &Workspace,
    conversation_id: &str,
    chat: &[ChatLine],
    query: &str,
) -> AgentContextSnapshot {
    compile_context_snapshot_with_budget(
        workspace,
        conversation_id,
        chat,
        query,
        ContextBudget::default(),
    )
}

pub fn compile_context_snapshot_with_budget(
    workspace: &Workspace,
    conversation_id: &str,
    chat: &[ChatLine],
    query: &str,
    budget: ContextBudget,
) -> AgentContextSnapshot {
    let budget = budget.bounded();
    let pinned_notes = load_state_for_conversation(workspace, conversation_id)
        .map(|state| normalize_context_notes(state.context_notes))
        .unwrap_or_default();
    let rolling_summary = build_rolling_summary(chat);
    let recent_start = chat.len().saturating_sub(budget.recent_message_limit);
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
        .take(budget.relevant_message_limit)
        .map(|(_, line)| context_message_from_line(line))
        .collect::<Vec<_>>();
    let recent_runs = load_agent_history_tail(workspace, budget.recent_run_limit)
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
        pinned_notes,
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
        if !meta.custom_title {
            meta.title = title;
        }
        meta.updated_at = now;
        meta.message_count = chat.len();
    }
    sort_index(index);
}

fn sort_index(index: &mut ConversationIndex) {
    index.conversations.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then(b.pinned.cmp(&a.pinned))
            .then(b.updated_at.cmp(&a.updated_at))
    });
}

fn next_available_conversation_id(index: &ConversationIndex) -> Option<String> {
    index
        .conversations
        .iter()
        .filter(|meta| !meta.archived)
        .max_by(|left, right| {
            left.pinned
                .cmp(&right.pinned)
                .then(left.updated_at.cmp(&right.updated_at))
        })
        .map(|meta| meta.id.clone())
}

fn sync_active_state(workspace: &Workspace, active_id: Option<String>) -> anyhow::Result<()> {
    let mut state = load_state(workspace);
    state.active_conversation_id = active_id;
    state.agent_state = None;
    state.updated_at = unix_timestamp();
    save_state(workspace, &state)
}

fn remove_conversation_files(workspace: &Workspace, conversation_id: &str) -> anyhow::Result<()> {
    for rel_path in [
        transcript_path(conversation_id),
        conversation_state_path(conversation_id),
    ] {
        let path = workspace.resolve_for_write(&rel_path)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn conversation_title(chat: &[ChatLine]) -> String {
    chat.iter()
        .find(|line| matches!(line.role, ChatRole::User) && !line.content.trim().is_empty())
        .map(|line| compact_inline(&line.content, 48))
        .unwrap_or_else(|| "Новый чат".to_string())
}

fn build_rolling_summary(chat: &[ChatLine]) -> String {
    let older_count = chat.len().saturating_sub(DEFAULT_RECENT_MESSAGE_LIMIT);
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

fn normalize_context_notes(notes: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for note in notes {
        let note = compact_inline(note.trim(), CONTEXT_NOTE_CHARS);
        if note.is_empty() || normalized.iter().any(|existing| existing == &note) {
            continue;
        }
        normalized.push(note);
        if normalized.len() >= CONTEXT_NOTE_LIMIT {
            break;
        }
    }
    normalized
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

fn safe_identifier(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(64)
        .collect::<String>();
    if sanitized.is_empty() {
        "profile".to_string()
    } else {
        sanitized
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

    #[test]
    fn renames_pins_archives_and_deletes_conversations() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let first = create_new_conversation(&workspace).expect("first");
        let second = create_new_conversation(&workspace).expect("second");

        let index = rename_conversation(&workspace, &first.id, "Рабочий диалог").expect("rename");
        let renamed = index
            .conversations
            .iter()
            .find(|meta| meta.id == first.id)
            .expect("renamed");
        assert_eq!(renamed.title, "Рабочий диалог");
        assert!(renamed.custom_title);

        let index = set_conversation_pinned(&workspace, &first.id, true).expect("pin");
        assert_eq!(index.conversations.first().unwrap().id, first.id);

        let loaded = archive_conversation(&workspace, &first.id).expect("archive");
        assert_ne!(loaded.id, first.id);
        assert!(
            loaded
                .index
                .conversations
                .iter()
                .find(|meta| meta.id == first.id)
                .unwrap()
                .archived
        );

        let restored = restore_conversation(&workspace, &first.id).expect("restore");
        assert_eq!(restored.id, first.id);
        assert!(
            !restored
                .index
                .conversations
                .iter()
                .find(|meta| meta.id == first.id)
                .unwrap()
                .archived
        );

        let loaded = delete_conversation(&workspace, &second.id).expect("delete");
        assert!(loaded
            .index
            .conversations
            .iter()
            .all(|meta| meta.id != second.id));
        assert!(loaded.index.conversations.iter().any(|meta| !meta.archived));
    }

    #[test]
    fn context_snapshot_respects_manual_budget() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let mut chat = vec![ChatLine::system("Система")];
        for index in 0..18 {
            chat.push(ChatLine::user(format!(
                "Сообщение {index} про бюджет контекста"
            )));
        }

        let snapshot = compile_context_snapshot_with_budget(
            &workspace,
            "chat-budget",
            &chat,
            "бюджет",
            ContextBudget {
                recent_message_limit: 3,
                relevant_message_limit: 2,
                recent_run_limit: 0,
            },
        );

        assert_eq!(snapshot.recent_messages.len(), 3);
        assert!(snapshot.relevant_messages.len() <= 2);
        assert!(snapshot.recent_runs.is_empty());
    }

    #[test]
    fn exports_and_imports_context_profile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let source = create_new_conversation(&workspace).expect("source");
        let target = create_new_conversation(&workspace).expect("target");
        let notes = vec![
            "Primary goal: self-improving coding agent".to_string(),
            "UX preference: Codex-like layout".to_string(),
        ];
        let budget = ContextBudget {
            recent_message_limit: 22,
            relevant_message_limit: 11,
            recent_run_limit: 7,
        };

        let rel_path =
            export_context_profile(&workspace, &source.id, "Agent context", &notes, budget)
                .expect("export");
        let abs_path = workspace.root().join(rel_path);
        let (state, imported_budget) =
            import_context_profile_file(&workspace, &abs_path, &target.id).expect("import");

        assert_eq!(imported_budget.recent_message_limit, 22);
        assert_eq!(imported_budget.relevant_message_limit, 11);
        assert_eq!(imported_budget.recent_run_limit, 7);
        assert_eq!(state.context_notes, notes);

        let snapshot = compile_context_snapshot_with_budget(
            &workspace,
            &target.id,
            &target.chat,
            "context",
            ContextBudget::default(),
        );
        assert!(snapshot
            .pinned_notes
            .iter()
            .any(|note| note.contains("self-improving")));
    }

    #[test]
    fn lists_exported_context_profiles() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let source = create_new_conversation(&workspace).expect("source");

        let rel_path = export_context_profile(
            &workspace,
            &source.id,
            "Portable context",
            &["Use project memory carefully".to_string()],
            ContextBudget {
                recent_message_limit: 10,
                relevant_message_limit: 5,
                recent_run_limit: 2,
            },
        )
        .expect("export");

        let profiles = list_context_profiles(&workspace);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].rel_path, rel_path);
        assert_eq!(profiles[0].profile.title, "Portable context");

        let profile = read_context_profile_file(&profiles[0].abs_path).expect("read profile");
        assert_eq!(profile.context_notes, vec!["Use project memory carefully"]);
    }

    #[test]
    fn context_snapshot_includes_pinned_notes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let loaded = create_new_conversation(&workspace).expect("create");
        save_conversation_context_notes(
            &workspace,
            &loaded.id,
            vec![
                "Главная цель: самосовершенствующийся агент".to_string(),
                "Главная цель: самосовершенствующийся агент".to_string(),
                "Не ломать UX Codex-style".to_string(),
            ],
        )
        .expect("save notes");

        let snapshot = compile_context_snapshot_with_budget(
            &workspace,
            &loaded.id,
            &loaded.chat,
            "что помнить",
            ContextBudget::default(),
        );

        assert_eq!(snapshot.pinned_notes.len(), 2);
        assert!(snapshot
            .to_prompt_block()
            .contains("самосовершенствующийся агент"));
    }
}
