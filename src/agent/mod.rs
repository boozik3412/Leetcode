pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod models;
pub mod openai;
pub mod provider;
pub mod routing;
pub mod subagent;
pub mod types;

use crate::agent::provider::{build_routed_provider, ProviderInput};
use crate::agent::routing::{
    describe_route_plan, resolve_task_route, route_candidates, route_name,
};
use crate::agent::types::{AppEvent, ToolResult};
use crate::config::AppConfig;
use crate::conversation::AgentContextSnapshot;
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::ToolDispatcher;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentState {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub provider_state: Option<serde_json::Value>,
}

impl AgentState {
    pub fn reset(&mut self) {
        self.provider_id = None;
        self.model_id = None;
        self.previous_response_id = None;
        self.provider_state = None;
    }
}

pub async fn run_user_turn(
    user_input: String,
    config: AppConfig,
    workspace: Option<Workspace>,
    state: Arc<Mutex<AgentState>>,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    context_snapshot: Option<AgentContextSnapshot>,
) -> anyhow::Result<()> {
    let policy = PolicyConfig::from_config(&config);
    let dispatcher = ToolDispatcher::new(
        workspace.clone(),
        config.clone(),
        events.clone(),
        approvals,
        cancel.clone(),
        policy,
    );
    let task = resolve_task_route(&config, &user_input);
    let candidates = route_candidates(&config, task);
    if candidates.is_empty() {
        anyhow::bail!(
            "Нет доступного маршрута провайдер/модель для задачи {}. Сохраните совместимый API-ключ или переключите маршрут на Авто/Код.",
            route_name(task)
        );
    }
    let _ = events.send(AppEvent::ToolOutput {
        id: "routing".to_string(),
        chunk: format!(
            "Маршрут задачи: {}\n{}",
            route_name(task),
            describe_route_plan(&candidates)
        ),
    });
    let mut candidate_index = 0usize;
    let mut provider = build_routed_provider(&config, &candidates[candidate_index])?;
    let mut provider_id = candidates[candidate_index].provider_id.clone();
    let mut model_id = candidates[candidate_index].model_id.clone();
    let (mut previous_response_id, provider_state) = {
        let state = state.lock().expect("agent state poisoned");
        if state.provider_id.as_deref() == Some(provider_id.as_str())
            && state.model_id.as_deref() == Some(model_id.as_str())
        {
            (
                state.previous_response_id.clone(),
                state.provider_state.clone(),
            )
        } else {
            (None, None)
        }
    };
    provider.import_state(provider_state)?;

    let workspace_text = workspace
        .as_ref()
        .map(|workspace| workspace.root().display().to_string())
        .unwrap_or_else(|| "рабочая папка не выбрана".to_string());
    let project_memory = format!(
        "{}\nПоддерживай память проекта актуальной: сохраняй устойчивые факты, внешние требования, brief/spec, решения и важные выдержки через record_memory_source, а затем учитывай эти источники при планировании и правках. Если для задачи нужен полный текст источников, сначала вызови memory_snapshot.",
        crate::memory::memory_summary_for_prompt(workspace.as_ref())
    );
    let task_memory_guidance = "При вызове upsert_task для проектных задач по возможности заполняй workstream, milestone и priority, чтобы Project Command Center показывал дерево направлений и этапов.";
    let asset_export_guidance = "При вызове export_asset можно передать target_dir, чтобы скопировать готовый ассет в папку проекта вроде assets/images, assets/audio, public/assets или src/assets.";
    let conversation_context = context_snapshot
        .as_ref()
        .map(AgentContextSnapshot::to_prompt_block)
        .unwrap_or_else(|| "Контекст переписки: сохранённая переписка не подключена.".to_string());
    let instructions = format!(
        "Ты Leetcode, лаконичный локальный агент для программирования внутри Windows desktop-приложения. \
Отвечай пользователю на русском языке. Текущий провайдер модели: {} ({}). \
Используй инструмент act, когда нужно изучить файлы, редактировать файлы, искать по коду, запускать проектные команды, запускать shell-команды, делать скриншот, управлять рабочим столом или генерировать ассеты. \
Все пути к файлам должны быть относительными к выбранной рабочей папке. Текущий корень рабочей папки: {workspace_text}. \
{conversation_context} \
{project_memory} {task_memory_guidance} {asset_export_guidance} \
Общий план нетривиальной задачи подтверждается интерфейсом Leetcode до запуска агентного цикла. Если запрос содержит текст о подтверждённом плане, не повторяй вопрос «правильно ли я понимаю» и сразу выполняй подтверждённый план; запрашивай подтверждения только для конкретных рискованных действий инструментов. \
Перед изменением кода сначала изучай релевантные файлы. Для многострочных правок предпочитай apply_patch, \
а edit_file используй только для небольших уникальных замен строк. Для типовых задач жизненного цикла проекта, таких как check, test, run, build, dev, preview, lint, editor или release, предпочитай project_command; для локальных preview-хуков браузера/приложения используй open_project_preview; для разовых нестандартных команд используй run_shell. Для постоянных интерактивных сессий, таких как dev-серверы, REPL, watcher'ы, логи игровых движков или команды, где важно сохранять cwd/env/session, используй terminal_start, terminal_write, terminal_read и terminal_stop. Перед изменением рискованной доступности инструментов используй governance_snapshot; set_tool_enabled, set_category_enabled и add_shell_deny_pattern применяй только когда пользователь просит изменить разрешения инструментов. Чтобы поддерживать память проекта актуальной, используй memory_snapshot, record_project_goal, upsert_task, update_task_status и record_decision. Для организации сгенерированных ассетов используй asset_library_snapshot, tag_asset, favorite_asset и export_asset_pack. Для локальных записей валидации используй run_replay_eval и eval_snapshot. Когда важен статус провайдера/ключа/модели, используй provider_health_snapshot. Для игровых и app-сценариев, таких как прототип механики, план спрайт-листа, UI-звуки, иконки предметов, вертикальный срез или чеклист плейтеста, используй game_workflow. Для крупных задач, которые затрагивают несколько доменов, много файлов, валидацию плюс реализацию или ассеты плюс код, сначала предложи пользователю компактный план субагентов вместо немедленного выполнения: назови роли, ограниченные задачи, ожидаемую пользу и попроси подтверждение. Если пользователь уже попросил продолжать, одобрил субагентов или использовал фразы вроде использовать субагентов/распараллелить/разбить задачу, вызывай run_subagent напрямую. Используй run_subagent, когда ограниченный специалист может выполнить небольшую часть работы и вернуть тебе выводы; выбирай code_agent, game_designer, art_director, audio_agent, qa_agent или build_agent и держи задачу сфокусированной. Используй delegate_agent только когда хочешь записать передачу без выполнения. Используй update_workspace_context для сохранения устойчивых фактов и решений проекта; record_run_summary — на полезных контрольных точках; orchestration_snapshot — перед планированием по ролям; export_trace или create_replay_eval — когда пользователь просит аудит или повторяемую валидацию. Для визуальных ассетов игр/приложений используй generate_image_asset, для анимационных листов — generate_spritesheet_asset, для UI/game-звуков или озвучки — generate_audio_asset, для коротких клипов — generate_video_asset. Для существующих задач изображений используй regenerate_image_asset или vary_image_asset, для следующих шагов пайплайна ассетов — upscale_asset/export_asset/attach_asset, для применения сгенерированной иконки — use_asset_as_app_icon, а open_asset_folder — когда пользователь хочет открыть сгенерированные ассеты. Для работы с рабочим столом предпочитай active_window и desktop_step: сначала наблюдение, затем при необходимости focus_window, потом один шаг click/type_text/hotkey со скриншотами до и после. Сырой screenshot, mouse_click, type_text или hotkey используй только для небольших прямых действий, когда активное окно и координаты уже понятны. Пользовательские объяснения держи короткими и конкретными.",
        provider.display_name(),
        provider.id()
    );

    let mut input = ProviderInput::Text(user_input);

    loop {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("Запуск отменён");
        }

        let streamed = match provider
            .stream_turn(
                &instructions,
                input.clone(),
                previous_response_id.as_deref(),
                &events,
            )
            .await
        {
            Ok(streamed) => streamed,
            Err(err) if previous_response_id.is_some() && is_previous_response_missing(&err) => {
                let _ = events.send(AppEvent::Error(format!(
                    "Сохранённый previous_response_id недоступен у провайдера; продолжаю через локальный transcript/context snapshot: {err}"
                )));
                {
                    let mut state = state.lock().expect("agent state poisoned");
                    state.previous_response_id = None;
                }
                provider
                    .stream_turn(&instructions, input.clone(), None, &events)
                    .await?
            }
            Err(err) if candidate_index + 1 < candidates.len() => {
                let failed = candidates[candidate_index].clone();
                candidate_index += 1;
                let next = candidates[candidate_index].clone();
                let _ = events.send(AppEvent::Error(format!(
                    "{} / {} не выполнен, переключаюсь на {} / {}: {}",
                    failed.provider_id, failed.model_id, next.provider_id, next.model_id, err
                )));
                provider = build_routed_provider(&config, &next)?;
                provider_id = next.provider_id;
                model_id = next.model_id;
                {
                    let mut state = state.lock().expect("agent state poisoned");
                    state.provider_id = None;
                    state.model_id = None;
                    state.previous_response_id = None;
                    state.provider_state = None;
                }
                provider
                    .stream_turn(&instructions, input.clone(), None, &events)
                    .await?
            }
            Err(err) => return Err(err),
        };
        previous_response_id = Some(streamed.response_id.clone());
        {
            let mut state = state.lock().expect("agent state poisoned");
            state.provider_id = Some(provider_id.clone());
            state.model_id = Some(model_id.clone());
            state.previous_response_id = Some(streamed.response_id.clone());
            state.provider_state = provider.export_state();
        }

        if !streamed.emitted_text {
            for text in streamed.text_chunks {
                let _ = events.send(AppEvent::AssistantText(text));
            }
        }

        let calls = streamed.tool_calls;
        if calls.is_empty() {
            return Ok(());
        }

        let mut tool_outputs = Vec::new();
        for call in calls {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("Запуск отменён");
            }

            let result = dispatcher.execute(&call).await;
            tool_outputs.push(json!({
                "type": "function_call_output",
                "call_id": call.call_id,
                "output": result.as_model_output()
            }));
        }

        if tool_outputs.is_empty() {
            let result = ToolResult::error("Инструменты не вернули вывод");
            input = ProviderInput::ToolOutputs(vec![json!({
                "type": "function_call_output",
                "call_id": "missing",
                "output": result.as_model_output()
            })]);
        } else {
            input = ProviderInput::ToolOutputs(tool_outputs);
        }
    }
}

fn is_previous_response_missing(err: &anyhow::Error) -> bool {
    let text = err.to_string().to_lowercase();
    text.contains("previous_response")
        || text.contains("previous response")
        || text.contains("response_not_found")
        || text.contains("not found")
}
