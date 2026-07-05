# Бэклог разработки Leetcode

Leetcode развивается из минимального локального помощника для программирования в настольную AI-студию для разработки игр и приложений. Продукт должен поддерживать нескольких AI-провайдеров, сценарии кодинга, генерацию ассетов, инструменты с учётом структуры проекта и в перспективе — многоагентную оркестрацию.

## Этап 1 - Надёжный рантайм для программирования

- Done: Stream Responses API events into the UI instead of waiting for full model turns.
- Done: Tighten tool schemas and validation so model arguments fail clearly and safely.
- Done: Add dry-run patch validation with `git apply --check` before applying patches.
- Done: Show patch impact and git status/diff in the UI.
- Done: Add a durable action journal for agent runs, approvals, shell commands, patch attempts, and failures.
- Done: Add Journal and Runtime UI panels for state visibility and audit review.
- Done: Add saved Safe/Normal/Strict/Custom policy profiles over shell/write approvals.
- Done: Add focused tests for workspace path safety, tool argument parsing, patch validation, and shell command construction.
- Done: Add a persistent terminal session with start/write/read/stop tools, UI panel, and shared state across agent turns.

## Этап 2 - Слой провайдеров моделей

- Done: Replace the direct `OpenAiClient` dependency in the runtime with a provider trait.
- Done: Add a model registry with capabilities: code, reasoning, tools, vision, image, audio, video, realtime, embeddings.
- Done: Keep OpenAI Responses as the default coding/reasoning provider.
- Done: Add provider settings and API key storage per provider.
- Done: Add implemented Claude, DeepSeek, and Gemini coding-agent providers.
- Done: Add task-based model routing: coding, planning, cheap/fast, vision, image generation, audio generation, video generation, and realtime.
- Done: Add fallback model/provider chains and clear provider error reporting.

## Этап 3 - Конвейер генерации ассетов

- Done: Introduce asynchronous image asset jobs with status, provider, model, prompt, parameters, output files, and metadata.
- Done: Add image asset previews in the tool panel.
- Done: Save generated image outputs into `assets/generated/images`.
- Done: Add image providers: ChatGPT Image, Nano Banana, Stability AI, and Replicate FLUX.
- Done: Expose image generation as the agent-callable `generate_image_asset` action.
- Done: Add asset cards with regenerate, variation, use-as-icon, open-folder, and load-prompt actions.
- Done: Add agent-callable asset follow-up actions: `regenerate_image_asset`, `vary_image_asset`, `use_asset_as_app_icon`, and `open_asset_folder`.
- Done: Track licensing/provider metadata for generated assets.
- Done: Add upscale, export, and attach-to-chat workflows.
- Done: Extend the same job framework to spritesheets, audio, video, and generated UI/game assets.
- Done: Expose spritesheet, audio, video, upscale, export, and attach workflows as agent-callable tools.

## Этап 4 - Профили проектов игр и приложений

- Done: Detect common project types: Rust, Node/package.json, React/Vite, Python, Godot, Unity, Unreal, and mixed repos.
- Done: Add project commands for run, test, build, lint, package script, editor, preview, and release where the project exposes them.
- Done: Add a Project panel with quick commands and streamed output through the existing tool log.
- Done: Expose quick commands as the agent-callable `project_command` action.
- Done: Add game-specific flows: prototype mechanic, generate spritesheet, generate UI sounds, create item icons, build vertical slice, run playtest checklist.
- Done: Add browser/app preview hooks for web and game tooling where available.

## Этап 5 - Многоагентная оркестрация

- Done: Add specialist roles: Code Agent, Game Designer, Art Director, Audio Agent, QA Agent, Build Agent.
- Done: Add handoff records and shared workspace context under `assets/generated/orchestration`.
- Done: Keep Rust-owned orchestration for the desktop MVP and document the future OpenAI Agents SDK sidecar path for richer hosted tracing, sessions, and independent specialist execution.
- Done: Add run summaries, trace export, replayable evaluation case files, and an `Agents` UI panel.
- Done: Expose orchestration as agent-callable tools: `run_subagent`, `delegate_agent`, `update_workspace_context`, `record_run_summary`, `export_trace`, `create_replay_eval`, and `orchestration_snapshot`.
- Done: Add bounded subagent execution with role-specific tool allowlists, max-round limits, approval-gated tool execution, and traceable `SubagentRun` records.
- Done: Instruct the manager agent to propose a subagent split for broad work before executing it, unless the user already approved using subagents.

## Этап 6 - Управление рабочим столом и компьютером

- Done: Add screenshot capture, click, type, hotkey, active-window inspection, and window focus tools.
- Done: Add approval-gated screenshot capture into `assets/generated/screenshots`.
- Done: Add approval-gated mouse click, text typing, and hotkey desktop tools.
- Done: Add `desktop_step` for the computer-use loop: screenshot, model action, execute, screenshot output.
- Done: Add a `Desktop` panel that shows the latest desktop step screenshot and active-window summary.
- Keep allowlists and human approval for irreversible desktop actions.

## Этап 7 - UX управления разрешениями

- Done: Add prompt-bar permission modes: Ask, Auto, Work, and Full.
- Done: Apply the selected mode across shell, file writes, paid asset APIs, desktop control, external opens, and orchestration tools.
- Done: Preserve backward compatibility for previous Safe/Normal/Strict/Custom saved configs.
- Done: Show the effective permission matrix in the Runtime panel.

## Этап 8 - Центр управления инструментами

- Done: Add a tool registry with category, risk, and description metadata.
- Done: Add workspace-level governance config for disabled tools, disabled categories, and shell deny patterns.
- Done: Enforce governance before every agent tool execution.
- Done: Add a Governance UI panel and agent-callable governance tools.

## Этап 9 - Память проекта / доска задач

- Done: Add persistent project memory under `assets/generated/leetcode/memory.json`.
- Done: Add goals, tasks, decisions, and prompt-time memory summaries.
- Done: Add a Memory UI panel for quick goal/task/decision capture.
- Done: Expose memory snapshot and update tools to the agent.
- Done: Add project memory sources for imported files and saved notes, including agent-callable source capture.

## Этап 10 - Менеджер библиотеки ассетов

- Done: Add an asset library index over generated asset jobs.
- Done: Support tags, favorites, notes, source job metadata, and export packs.
- Done: Add an Asset Library UI panel with filtering and favorite toggles.
- Done: Expose asset library snapshot/tag/favorite/export tools to the agent.

## Этап 11 - Валидация и eval-сценарии

- Done: Add static replay eval runner over existing orchestration eval cases.
- Done: Persist eval run results under `assets/generated/leetcode/eval_results.json`.
- Done: Add an Evals UI panel and agent-callable eval snapshot/run tools.

## Этап 12 - Панель состояния моделей и провайдеров

- Done: Add offline provider health reporting for chat and asset providers.
- Done: Check configured keys, selected models, registry coverage, and provider issues.
- Done: Add a Providers UI panel and agent-callable provider health snapshot.

## Этап 13 - Полировка UX в стиле "Codex"

- Done: Add clear interface modes: Chat, Code, Assets, and Project.
- Done: Scope the right sidebar by the selected mode so it does not show every tool group at once.
- Done: Keep file tabs and agent chat as first-class central surfaces.
- Done: Restyle the app shell and Context panel after a real screenshot pass: softer dark theme, clearer right-panel titles, compact context health dashboard, visual composition bars, and folded technical prompt preview.
- Done: Add daily-mode UX polish after real screenshots: compact AI settings menu in the top bar, quieter input bar, helpful empty agent home state, narrower right sidebar, and explanatory tooltips for context controls.
- Done: Simplify daily-work UI pass: combined AI selector in the top bar, project chip, flatter prompt bar, and richer empty agent home shortcuts for project, roadmap, release, and assets.
- Done: Add saved workspace view presets for chat, code, roadmap, release, and assets, including persisted left-project-panel collapsed state and restored right-panel selection.
- Todo: Continue visual simplification after real screenshots from daily usage.

## Этап 14 - Центр команд проекта

- Done: Promote project profiles, quick commands, preview hooks, terminal state, and recent command output into a command-center view.
- Done: Add first-pass rerun and "ask agent to fix" actions for project command failures.
- Done: Add command-run records with status, timestamp, exit code, command, and compact stdout/stderr tail.
- Done: Add "fix", "rerun", and "open preview" actions tied to concrete command runs.
- Done: Build fix prompts from the exact failed command output so the agent receives actionable diagnostics.
- Done: Add lightweight build-error grouping for failed command output.
- Done: Add structured diagnostics by compiler/test framework with file, line, column, type, targeted fix prompts, and fix-request history.
- Done: Add richer task tree across project milestones and launch/game-development workstreams.

## Этап 15 - Студия ассетов

- Done: Promote asset generation and asset history into a dedicated workspace mode.
- Done: Show generated asset history, variants, favorites, export, attach, and source metadata from one place.
- Done: Add side-by-side comparison for multiple generated results.
- Done: Add project import targets for common game/app asset folders.

## Этап 16 - Проверка реальных провайдеров

- Done: Add a validation checklist for OpenAI, Claude, DeepSeek, Gemini, image, audio, and video providers.
- Done: Add safe manual live validation for chat providers through the real provider layer: model text response plus tool-call shape check.
- Done: Run provider validation through the configured HTTP/proxy client.
- Done: Add explicit paid smoke buttons for image, audio, and video providers that launch small Asset Studio jobs only after a user click.
- Done: Keep recent provider validation results visible in the Providers panel.
- Done: Persist provider validation results under `assets/generated/leetcode/provider_validation_results.json`.

## Этап 17 - Упаковка и дистрибуция

- Done: Add Windows packaging docs and a portable build script.
- Done: Add runtime environment diagnostics for executable/config/journal/workspace paths, proxy state, and local toolchain availability.
- Done: Expose environment diagnostics as the read-only `environment_snapshot` agent tool.
- Done: Add release checklist for icon, version, README, provider setup, diagnostics, and workspace/config storage.
- Done: Document current crash/log folder policy.
- Done: Add signed portable archive path with zip output, SHA256 manifest, and optional SignTool support.
- Done: Add optional crash report folder and panic hook.
- Done: Add Release Cockpit in the right workspace panel with version summary, preflight checklist, release/package commands, artifact discovery, and publication diagnostics.
- Done: Expose the Windows package script as a detected project command when `scripts/package-windows.ps1` is present.
- Done: Add Release Cockpit action that records the current release candidate, readiness checklist, artifacts, validations, Git context, and release command history into Roadmap as a milestone.

## Этап 18 - Управляемая автономность и таймлайн запуска

Цель: превратить богатый набор инструментов Leetcode в прозрачный управляемый рабочий процесс: пользователь видит план, текущий шаг, подтверждения, проверки и итоговый отчёт без необходимости собирать картину из разных панелей.

- Done: Added an MVP compact run timeline under the current/last assistant message with planning, assistant output, tool calls, approvals, failure/cancellation, and completion states.
- Done: Added app-owned pre-run plans for non-trivial tasks: the UI shows the agent's understanding and intended approach before the model/tool loop starts.
- Done: Added an enforced pre-run confirmation gate with a single `Подтверждаю` action; user clarifications in the input rebuild the plan without launching tools.
- Done: Expanded aggregation beyond live agent events to include richer journal, project command records, orchestration summaries, and eval result links in one readable run narrative.
- Done: Added expandable tool-call detail cards with action/status/duration/compact output for timeline steps.
- Done: Added an automatic final run report with changed files, commands executed, validation results, risks, next steps, and saved run summary id placeholder.
- Done: Added reusable `src/run_timeline.rs` module for timeline data structures and aggregation helpers.
- Done: Added replay/static eval case for timeline-critical behaviours and ran static replay validation.
- Done: Added UI polish for daily use: default horizontal alignment stays left-aligned and long content remains constrained between side panels.

### Предлагаемый MVP-срез

1. Done: Introduce `RunTimeline`, `RunTimelineStep`, `RunTimelineStatus`, and event mapping from existing agent/tool/project events.
2. Done: Render a left-aligned collapsible timeline under the active assistant message.
3. Done: Populate timeline from current live status plus completed tool/project command records.
4. Done: Generate a final compact report when the agent run finishes.
5. Done: Validate with `cargo check`, `cargo test`, and one replay eval case.

## Этап 19 - Постоянная переписка и управляемый контекст

Цель: сделать диалоги и рабочий контекст агента долговечными: пользователь должен продолжать работу после перезапуска приложения, а агент должен получать компактную, управляемую память предыдущей переписки без переполнения окна модели.

- Done: Add workspace-local conversation storage under `assets/generated/leetcode/conversations`.
- Done: Persist each chat as JSONL transcript plus per-chat runtime state, including provider/model response state when available.
- Done: Restore the active conversation and agent state at app startup and when switching projects.
- Done: Add central chat controls for creating a new chat and switching between saved chats.
- Done: Save user messages, pre-run plans, confirmations, approval decisions, assistant responses, errors, and completion metadata as the chat changes.
- Done: Add a context compiler that injects rolling summary, recent messages, relevant older messages, and recent run history into every agent turn.
- Done: Keep provider `previous_response_id` as an optimization while falling back to local transcript/context when the provider-side id is unavailable.
- Done: Add tests for transcript restore and relevant-message retrieval.
- Done: Add explicit chat rename, pin, archive, and delete controls.
- Done: Add a dedicated context inspector so the user can see exactly what summary/retrieval block will be sent to the model.
- Done: Add archived-chat browser with restore and delete actions.
- Done: Add manual context budget controls for recent messages, retrieved messages, and run-history count.
- Done: Add context presets like "short", "balanced", and "deep research".
- Done: Add per-chat context notes that can be pinned into every future prompt.
- Done: Add context profile export/import between projects.
- Done: Add automatic suggestions for new pinned notes after successful long agent runs.

## Этап 20 - Центр управления контекстом

Цель: собрать все механики долговременного контекста в один понятный модуль, чтобы пользователь видел, что именно агент помнит, какие источники подключены и какой профиль контекста будет использоваться в следующем запуске.

- Done: Add a dedicated Context Control Center surface that unifies chats, pinned notes, imported memory sources, context profiles, context budgets, and recent run history.
- Done: Show profile import/export history and make exported profiles easy to reuse across projects.
- Done: Add preview/diff for context profile import before applying it to the active chat.
- Done: Add quick actions for pinning notes from selected chat messages, run summaries, roadmap items, and project memory sources.
- Done: Add context health indicators: approximate prompt size, stale notes, duplicate facts, missing project goals, and oversized sources.

## Этап 21 - Безопасное самоизменение агента

Цель: сделать самоулучшение Leetcode управляемым и обратимым: перед изменением собственного кода агент должен фиксировать намерение, точку восстановления, проверки и понятный итог.

- Done: Add a self-modification workflow that creates a restore snapshot before changing files in the Leetcode project itself.
- Done: Reuse the confirmed pre-run implementation plan as the self-modification gate before the agent starts editing Leetcode.
- Done: Run `cargo fmt`, `cargo check`, and `cargo test` automatically after self-modification changes.
- Done: Detect validation-breaking changes and show rollback/targeted-repair guidance with the restore snapshot path.
- Done: Save self-modification snapshots and validation output into the run timeline and agent history.
- Done: Link self-modification snapshots and validation output to structured roadmap milestones through roadmap link fields and `record_milestone`.

## Этап 22 - Живая дорожная карта проекта

Цель: превратить roadmap в живую историю развития проекта: прошлые изменения, текущая работа, будущие этапы и финальное видение должны быть связаны с коммитами, агентными запусками и памятью проекта.

- Done: Move roadmap data into `src/roadmap.rs` with workspace-local JSON state under `assets/generated/leetcode/roadmap.json`, while keeping `BACKLOG.md` as readable documentation and seed source.
- Done: Add agent-callable tools to record milestones, start/current work, complete roadmap items, plan future stages, read snapshots, and export markdown.
- Done: Link roadmap items to commits, changed files, agent history records, project memory ids, and validation results through structured link fields.
- Done: Add UI controls for filtering roadmap by done/current/next, editing item status, and exporting roadmap snapshots.
- Done: Keep final product goals visible as the last section of the roadmap.
- Done: Link Release Cockpit checkpoints into Roadmap milestones so releases become part of the living project history.

## Этап 23 - Проводник истории агента

Цель: сделать историю действий агента полноценным инструментом анализа: пользователь должен быстро понимать, что агент делал, почему, какими инструментами пользовался, где ошибался и что изменил.

- Done: Add a searchable Agent History Explorer with filters by status, free-text provider/model/tool/file search, duration, and date period.
- Done: Show each run as a readable report with confirmed plan, tool calls, approvals, changed files, validation, errors, and final response.
- Done: Add actions to replay a run as an eval case, turn run output into project memory, or attach it to a roadmap milestone.
- Done: Add compact analytics for run duration, tool usage, failures, and provider/model reliability.
- Done: Add Markdown export for selected runs and filtered history for external review.

## Этап 24 - Палитра команд и навигация

Цель: дать Leetcode единый быстрый вход ко всем основным действиям, чтобы пользователь мог управлять агентом, проектом, панелями и prompt-шаблонами без поиска по интерфейсу.

- Done: Add a command palette opened by `Ctrl+K`, `Ctrl+Shift+P`, or the top-bar `Команды` button.
- Done: Add searchable commands grouped by view, navigation, project, agent, Git, panels, prompts, and detected project commands.
- Done: Wire palette actions to existing safe app logic: layout presets, project open/refresh, chat reset/new chat, Git status/commit, project commands, prompt templates, and stop actions.
- Done: Persist recent command history in app config and rank recent commands higher in search results.
- Done: Persist favorite commands and expose a quick favorite toggle for the selected palette item.
- Done: Add user-defined command macros created from favorites and executable from the same palette.
- Done: Add macro editing, step reordering/removal, JSON import/export, and optional per-step confirmation for advanced workflows.

## Этап 25A - Remote Control Foundation

Цель: заложить безопасный приватный фундамент удалённого доступа к уже запущенному локальному агенту без AnyDesk/VNC.

- Done: Составить двухэтапный план remote control в `docs/remote-control-plan.md`.
- Done: Добавить настройки Remote API в локальный конфиг: включение, host, port, access token.
- Done: Добавить встроенный локальный Remote API server, выключенный по умолчанию.
- Done: Добавить token-protected `GET /api/state` и `GET /api/events` для live-состояния агента.
- Done: Добавить `GET /health`, `GET /` и `GET /manifest.webmanifest` для проверки и мобильной PWA-панели.
- Done: Добавить UI в панель контроля: запуск/остановка, перезапуск, новый token, URL и статус.
- Todo: Добавить ручную инструкцию для Tailscale Serve / Cloudflare Tunnel после тестирования на реальной машине.

## Этап 25B - Product Remote Access

Цель: превратить foundation в полноценный сценарий удалённого управления с тонким клиентом, iPhone-доступом и подключением по Agent ID.

- Todo: Спроектировать Leetcode Relay: исходящее WSS-соединение от host app, Agent ID, device pairing и revoke devices.
- Todo: Добавить pairing flow: одноразовый код/QR, подтверждение на host app, device token.
- Done: Добавить local-first remote action API: `POST /api/tasks`, `POST /api/run-gate`, `POST /api/approval`.
- Done: Подключить PWA-форму отправки задачи и кнопки подтверждения/отклонения к существующей permission system.
- Done: Добавить remote observer endpoints: `GET /api/tool-log`, `GET /api/history`, `GET /api/files`.
- Done: Добавить read-only просмотр UTF-8 файлов внутри текущей workspace через `GET /api/files/content?path=...`.
- Done: Добавить PWA-блок "Наблюдение" для логов, истории запусков и файлов.
- Done: Расширить observer до подробного run timeline через `GET /api/history/run?id=...` и PWA drill-down по выбранному запуску.
- Done: Добавить security baseline для local Remote API: роли доступа, Origin allowlist, глобальный rate limit и audit-события удалённых действий.
- Done: Добавить safe Remote Commands: `GET /api/commands`, `POST /api/commands`, PWA-кнопки и app-side allowlist для безопасных команд палитры.
- Todo: Расширить Remote Commands до макросов/проектных команд с отдельными ролями, подтверждениями и риск-классами.
- Todo: Усилить безопасность до product-grade: short-lived tokens, device pairing, revoke devices, TLS/WSS, per-device/per-IP rate limiting.
- Todo: Сделать iPhone-first PWA и позже решить, нужен ли native iOS-клиент.
- Done: Сделать лёгкий desktop thin client как отдельный `leetcode-client` binary поверх Remote API: состояние агента, отправка задач, safe commands, approvals.
- Done: Добавить installer/portable package для тонкого клиента: `package-client-windows.ps1`, `install-leetcode-client.ps1`, `uninstall-leetcode-client.ps1`, `client-latest.json`.
- Todo: Довести thin client до Agent ID/pairing UX после появления relay/device registry.

## Этап 26 - Installer и обновления

Цель: превратить Leetcode из dev/portable-сборки в устанавливаемый продукт, который можно поставить на другой компьютер и обновлять без ручной сборки из репозитория.

- Done: Добавить Windows installer foundation: install/uninstall scripts, установка в user-local app directory, ярлыки Start Menu/Desktop и install metadata.
- Done: Расширить packaging script так, чтобы он собирал portable package, installer bundle и update manifest `latest.json`.
- Done: Добавить Release Cockpit/diagnostics видимость installer/update артефактов.
- Done: Добавить ручную проверку обновлений по manifest URL.
- Done: Добавить автопроверку обновлений при запуске: Leetcode читает `latest.json`, сравнивает версию и показывает статус без автоматической установки.
- Done: Добавить безопасное скачивание обновления с SHA256-проверкой.
- Done: Добавить обновление по кнопке: скачать пакет, проверить SHA256, запустить внешний updater, закрыть текущий процесс и перезапустить Leetcode.
- Done: Подготовить release-channel `0.1.1`: bump версии, сборка portable package, GitHub Release assets и smoke-проверка обнаружения обновления с установленной `0.1.0`.
- Done: Добавить отдельный release package foundation для тонкого клиента Windows.
- Todo: Добавить product-grade автообновление после стабилизации release канала, подписи, rollback UI и staged rollout.

## Этап 27 - Agent ID и простое подключение

Цель: спрятать IP/port/token-схему за понятной моделью: пользователь видит Agent ID, подключает тонкий клиент или iPhone и подтверждает устройство на основном компьютере.

- Done: Добавить постоянный `Agent ID`, который создаётся при первом запуске и сохраняется в конфиге.
- Done: Показать `Agent ID` в UI удалённого доступа с копированием и понятным статусом.
- Todo: Добавить device registry: доверенные устройства, роли, last seen, revoke.
- Todo: Добавить pairing flow: одноразовый код/QR, подтверждение на host app, device token.
- Done: Подготовить thin-client протокол поверх уже существующего Remote API для прямого URL/token подключения.
- Todo: Подготовить Relay/Agent ID слой: исходящее WSS-соединение от агента, подключение клиента по Agent ID без ручных IP/портов.

## Постоянная планка качества

- Every tool that mutates files, runs shell commands, calls paid APIs, or controls the desktop must have policy checks and visible audit logs.
- Every provider integration must include a minimal mocked test and a documented manual validation path.
- UI should make agent state visible: thinking/streaming, tool running, waiting for approval, cancelled, failed, or complete.
