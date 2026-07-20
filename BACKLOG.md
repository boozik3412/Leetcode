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

- Done: Спроектировать Leetcode Relay MVP: исходящий poll от host app, Agent ID, device pairing и revoke devices.
- Done: Добавить pairing flow: одноразовый код/QR, подтверждение на host app, device token.
- Done: Добавить local-first remote action API: `POST /api/tasks`, `POST /api/run-gate`, `POST /api/approval`.
- Done: Подключить PWA-форму отправки задачи и кнопки подтверждения/отклонения к существующей permission system.
- Done: Добавить remote observer endpoints: `GET /api/tool-log`, `GET /api/history`, `GET /api/files`.
- Done: Добавить read-only просмотр UTF-8 файлов внутри текущей workspace через `GET /api/files/content?path=...`.
- Done: Добавить PWA-блок "Наблюдение" для логов, истории запусков и файлов.
- Done: Расширить observer до подробного run timeline через `GET /api/history/run?id=...` и PWA drill-down по выбранному запуску.
- Done: Добавить security baseline для local Remote API: роли доступа, Origin allowlist, глобальный rate limit и audit-события удалённых действий.
- Done: Добавить safe Remote Commands: `GET /api/commands`, `POST /api/commands`, PWA-кнопки и app-side allowlist для безопасных команд палитры.
- Done: Расширить Remote Commands до макросов/проектных команд с отдельными ролями run/desktop, подтверждениями и риск-классами.
- Done: Усилить security baseline до управляемых trusted devices: token expiry, revoke, rotation, роли по устройствам и per-device/per-IP rate limiting.
- Done: Добавить local-first signed short-lived sessions для Remote API: `POST /api/sessions`, HMAC-подпись, TTL 15 минут и совместимость со старым access/device token.
- Done: Добавить отдельные remote-роли run/desktop для direct/relay commands, trusted devices, thin client и iPhone PWA preview.
- Done: Добавить relay-issued short-lived sessions: `/api/clients/sessions`, HMAC-подпись, TTL 15 минут, thin client/PWA используют session token с fallback на device token.
- Todo: Довести transport security до product-grade: TLS/WSS, публичный relay deployment и staged session policy hardening.
- Done: Сделать iPhone-first PWA и позже решить, нужен ли native iOS-клиент.
- Done: Сделать лёгкий desktop thin client как отдельный `leetcode-client` binary поверх Remote API: состояние агента, отправка задач, safe commands, approvals.
- Done: Добавить installer/portable package для тонкого клиента: `package-client-windows.ps1`, `install-leetcode-client.ps1`, `uninstall-leetcode-client.ps1`, `client-latest.json`.
- Done: Довести thin client до Agent ID/pairing UX после появления relay/device registry.

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
- Done: Добавить device registry: доверенные устройства, роли, last seen и revoke в основном Leetcode.
- Done: Добавить local-first pairing flow: одноразовый код на host app, подключение thin client, отдельный device token.
- Done: Подготовить thin-client протокол поверх уже существующего Remote API для прямого URL/token подключения.
- Done: Обновить thin client: подключение по Remote URL + Agent ID + pairing code, автоматическое сохранение device token.
- Done: Добавить UX pairing-passport: host app копирует Remote URL + Agent ID + одноразовый код, thin client вставляет эти данные из буфера.
- Done: Добавить управление ролями доверенных устройств прямо в UI host app: обзор, задачи, подтверждения, файлы.
- Done: Добавить QR/pairing link для iPhone/PWA после стабилизации relay/pwa маршрута.
- Done: Добавить host-side approval dialog для новых устройств, когда появится async pairing через relay.
- Done: Подготовить Relay/Agent ID foundation: исходящий poll от агента, подключение клиента через relay по Agent ID без прямого Remote URL.

## Этап 28 - Relay MVP

Цель: дать рабочий прототип подключения по Agent ID без входящего порта на машине агента: host app сама делает исходящий poll на relay, thin client отправляет задачи и approvals через relay.

- Done: Добавить общий relay protocol module: host poll, pairing, client state, tasks, commands, approvals.
- Done: Добавить отдельный `leetcode-relay` HTTP binary для локального/VPS relay MVP.
- Done: Добавить host-side relay settings: включение, relay URL, host token, статус синхронизации.
- Done: Добавить периодический outgoing relay poll из основного Leetcode и преобразование relay actions в обычные remote actions.
- Done: Добавить relay mode в `leetcode-client`: Agent ID + device token, pairing через relay, state polling, задачи, команды и approvals.
- Done: Включить `leetcode-relay.exe` в portable package тонкого клиента.
- Done: Добавить relay-issued client sessions поверх device token: session token не сохраняется на диск, TTL 15 минут, старые device-token запросы совместимы.
- Todo: Заменить HTTP long-poll на WSS/TLS relay с reconnect/backoff и server-side expiry.
- Done: Добавить host-side approval dialog для новых relay devices вместо автопары по коду.
- Todo: Добавить публичный relay deployment/update channel или настройку своего relay URL через installer.
- Done: Добавить iPhone/PWA pairing link/QR поверх relay.

## Этап 28B - Relay Hardening

Цель: превратить relay MVP в понятный и диагностируемый режим подключения, где пользователь видит online/offline статус агента, а клиент не выглядит "подключённым" к давно исчезнувшему host.

- Done: Добавить TTL host-сессии на relay: если host app давно не делала poll, клиент видит "агент offline".
- Done: Возвращать в relay state метаданные доступности: `host_online`, `host_updated_at`, `host_age_secs`, размер очереди действий.
- Done: Показывать в thin client источник подключения: direct или relay, online/offline host, возраст последнего snapshot.
- Done: Показывать в host app последнюю успешную relay-синхронизацию, ошибку и число полученных действий.
- Done: Добавить ручную relay E2E-инструкцию и smoke-проверку для локального relay.

## Этап 29 - iPhone/PWA Remote

Цель: дать простой доступ с телефона без полноценного desktop-клиента.

- Done: Сделать mobile-first PWA поверх relay: статус агента, отправка задачи, approvals, история запусков.
- Done: Добавить pairing link в host app для iPhone: ссылка открывает Relay PWA с Agent ID и временным pairing code.
- Done: Ограничить мобильный UI безопасными действиями и отдельными ролями: view/chat/approve без файлового доступа по умолчанию.
- Done: Добавить QR-код для iPhone поверх pairing link без внешнего сервиса.

## Этап 30 - Product-grade Pairing

Цель: убрать автопару по коду и сделать понятное подтверждение новых устройств на основном компьютере.

- Done: Сделать pending device requests: клиент отправляет запрос, host app показывает блок "подтвердить/отклонить".
- Done: Добавить редактирование имени устройства, сроков действия token и роли по умолчанию.
- Done: Добавить базовый per-device audit и быстрый revoke с видимым последствием для клиента.
- Done: Добавить срок действия/rotation device tokens и отдельную диагностику истёкших/отозванных устройств.

## Этап 31 - Remote Commands и макросы

Цель: разрешить удалённо запускать не только базовые команды, но и безопасные проектные сценарии.

- Done: Расширить Remote Commands до макросов/проектных команд с risk-class, confirmation и approve-gate для high-risk действий.
- Done: Добавить preview действия перед запуском команды через relay/PWA и thin client.
- Done: Привязать remote macros к существующей permission system, журналу действий и tool log.
- Done: Разделить удалённые права запуска: project commands, Git commit и macros требуют роль `run`; future desktop-control зарезервирован под роль `desktop`.

## Этап 32 - Observability и диагностика

Цель: чтобы пользователь понимал, что делает агент, где он завис и почему удалённое подключение не работает.

- Done: Добавить панель диагностики relay/direct/PWA: URL, latency, last sync, stale/offline reasons, device roles и подсказки по исправлению.
- Done: Добавить экспорт remote debug bundle без секретов.
- Done: Добавить structured event timeline для remote sessions.

## Этап 33 - Простое подключение тонкого клиента

Цель: убрать технические детали из обычного сценария удалённого доступа. Пользователь тонкого клиента нажимает `Добавить соединение`, вводит только `Agent ID`, а основной Leetcode показывает входящий запрос и даёт принять или отклонить устройство.

- Done: Сделать главный сценарий в Leetcode Client: `Добавить соединение` -> `Agent ID` -> `Подключиться`, без ручного Remote URL, host token, pairing passport и pairing code.
- Done: Оставить старый passport/pairing-code flow в раскрытом блоке `Дополнительно` для локальной диагностики и совместимости.
- Done: Добавить relay-запрос подключения без pairing code: клиент создаёт pending request по Agent ID, но не получает device token до явного решения host app.
- Done: В основном Leetcode показывать заметный входящий запрос подключения: имя устройства, время, источник, запрошенные роли и срок действия.
- Done: Добавить решения host app: `Принять один раз`, `Принять и запомнить`, `Отклонить`.
- Done: При `Принять и запомнить` сохранять устройство в trusted devices для последующих автоматических подключений.
- Done: При `Принять один раз` выдавать короткоживущий доступ без превращения устройства в долгосрочно доверенное.
- Done: Добавить авто-проверку статуса pending-запроса в thin client, чтобы после принятия на host клиент сам завершал подключение.
- Done: Записывать все события простого подключения в remote timeline: запрос, принятие, запоминание, отказ, истечение, повторное подключение.
- Done: Обновить README с короткой инструкцией: как подключиться по Agent ID из тонкого клиента.

## Этап 34 - Visual Project Map Foundation

Цель: построить машинно-читаемую карту проекта: узлы, связи, источники уверенности и связь с файлами, roadmap, памятью и командами.

- Done: Добавить `src/project_graph.rs` и workspace-local состояние `assets/generated/leetcode/project_graph.json`.
- Done: Описать типы узлов: Project, Folder, File, Module, Symbol, Command, Asset, Memory, RoadmapItem.
- Done: Описать типы связей: contains, imports, depends_on, calls, generates, tests, documents, related_to.
- Done: Реализовать первый scanner: файловое дерево, project profiles, команды проекта, roadmap items и memory sources.
- Done: Добавить agent-callable snapshot выбранной карты проекта.

## Этап 35 - Интерактивная карта проекта

Цель: дать пользователю визуальную навигацию по проекту и дать агенту точный selected-node context.

- Done: Добавить режим `Карта` рядом с `Чат`, `Код`, `Ассеты`, `Проект`.
- Done: Реализовать canvas с pan/zoom, поиском и фильтрами по типам узлов.
- Done: По клику на узел показывать правую панель: описание, путь, связи, последние изменения и быстрые действия.
- Done: Добавить действия `Открыть`, `Спросить агента`, `Добавить связь`, `Скрыть`, `Закрепить`.
- Done: Передавать выбранный узел в контекст агента, чтобы пользователь мог сказать “улучши это” после клика по узлу.

## Этап 36 - Self-Improvement Kernel

Цель: превратить безопасное самоизменение в измеримый экспериментальный цикл: гипотеза, baseline, изменение, проверка, решение и возможность отката.

- Done: Добавить workspace-local журнал экспериментов `assets/generated/leetcode/self_improvement/experiments.json`.
- Done: Автоматически создавать эксперимент для self-modification запуска и связывать его с restore snapshot и текущим Git HEAD.
- Done: Записывать критерии успеха, изменённые файлы, результаты `cargo fmt`, `cargo check`, `cargo test` и baseline replay-evals.
- Done: Добавить agent-callable действия `self_improvement_snapshot`, `start_self_improvement_experiment`, `decide_self_improvement_experiment` с governance и approval.
- Done: Не разрешать принимать эксперимент до успешного quality gate; сохранять явное решение и его обоснование.
- Done: Показать эксперименты и ожидающие решения рядом с локальными проверками в UI.
- Done: Добавить управляемый Git worktree/ветку в `%LOCALAPPDATA%/Leetcode/self-improvement/worktrees` и запретить её создание при грязной основной копии или изменившемся HEAD.
- Done: Добавить отдельный patch-инструмент, который может менять файлы только внутри записанного worktree кандидата.
- Done: Добавить benchmark registry с исполняемыми сценариями и сравнением baseline/candidate, включая обязательные `format`, `check`, `test`.
- Done: Реализовать явный workflow `candidate -> benchmarks -> accept/reject -> fast-forward promotion`.
- Done: Реализовать rollback продвинутого эксперимента через отдельный `git revert` и управляемую очистку worktree/ветки.
- Done: Добавить интеграционный тест полного Git-цикла на временном репозитории.
- Done: Сделать изолированный workflow обязательным по умолчанию для всех self-modification запусков и блокировать прямые mutating tools основной копии, пока существует активный candidate.

## Этап 37 - Unreal Engine 5.8 Bridge

Цель: дать агенту надёжный локальный мост к Unreal Engine 5.8 и стандартным инструментам сборки проекта.

- Done: Обнаруживать установки Unreal Engine, Engine Association проекта, плагины и доступные toolchain-компоненты.
- Done: Добавить безопасные профили UnrealEditor, UnrealEditor-Cmd, UBT и UAT.
- Done: Реализовать команды generate project files, build, editor, automation tests, cook, package и project validation.
- Done: Парсить AutomationTool, compiler и Unreal logs в структурированные ошибки с файлами и предлагаемыми действиями.
- Done: Добавить тестовый Unreal fixture project и ручной validation checklist для UE 5.8.

## Этап 38 - MCP Runtime и Unreal MCP

Цель: подключать локальные и удалённые MCP-серверы как управляемые возможности агента, начиная с официального Unreal MCP.

- Done: Добавить MCP client runtime для stdio и Streamable HTTP с lifecycle, reconnect и capability discovery.
- Done: Добавить реестр серверов, per-server permissions, allowed tools, approvals, timeout и audit trail.
- Done: Подключить Unreal Engine 5.8 MCP Server и показать его состояние в интерфейсе проекта.
- Done: Защитить MCP-ответы и tool metadata как недоверенный внешний контент и ограничить prompt injection.

## Этап 39 - Unreal Project Intelligence

Цель: расширить карту проекта до понимания модулей, ассетов и игровых связей Unreal.

- Done: Сканировать `.uproject`, `.uplugin`, `Build.cs`, `Target.cs`, Config и Source.
- Done: Импортировать Asset Registry/dependency export для карт, Blueprints, Data Assets, материалов, Niagara и анимаций.
- Done: Добавить Unreal-типы узлов и связей в Project Map с инкрементальным обновлением.
- Done: Передавать выбранный Unreal-узел агенту и MCP-инструментам как точный контекст задачи.

## Этап 40 - 3D Asset Pipeline

Цель: дать агенту воспроизводимый путь от идеи или изображения до готового игрового 3D-ассета в Unreal.

- Done: Добавить provider layer Meshy/Tripo для text/image-to-3D API и асинхронных jobs.
- Done: Поддержать GLB/glTF, FBX и USD, PBR texture sets, материалы, LOD, Nanite и collision metadata.
- Done: Добавить валидацию геометрии, масштаба, UV, текстур, лицензии и provenance до импорта.
- Done: Автоматизировать импорт и переимпорт через Unreal Interchange/Python с MCP-контекстом выбранного узла.
- Done: Добавить rig, skeleton и animation pipeline как отдельные проверяемые стадии.
- Done: Встроить 3D jobs в Asset Studio, инструменты агента и Project Map.

## Этап 41 - Gameplay и Level Pipeline

Цель: покрыть агентом основной цикл создания игрового вертикального среза в Unreal Engine.

- Done: Управлять картами, Actors, Components и Data Assets через безопасный Unreal Python manifest; Blueprint/C++ и сложные графы направлять в Unreal MCP/toolchain.
- Done: Добавить recipes для Gameplay Framework, PCG, Niagara, Enhanced Input, HUD и типовых игровых механик.
- Done: Автоматизировать Automation/map smoke/Movie Render Queue, сбор отчётов, скриншотов, визуальных/видео-артефактов и логов.
- Done: Связать gameplay-планы, задачи, roadmap, выбранный Project Map node и конкретные playtest-запуски.

## Этап 42 - Production Validation

Цель: сделать игровой и self-improvement пайплайн воспроизводимым перед релизом.

- Done: Добавить live contract tests провайдеров и MCP-серверов с явным opt-in.
- Done: Добавить headless Unreal automation, cook/package smoke и golden snapshots карты проекта.
- Done: Добавить screenshot regression для основных desktop/remote UI-сценариев.
- Done: Проверять installer, updater, relay и thin client в едином release preflight.

## Этап 43 - Game Production Director

Цель: связать полный цикл разработки игры на Unreal Engine 5.8 в один живой производственный план, где агент понимает масштаб проекта, текущий milestone, потоки работ, зависимости, артефакты и критерии перехода дальше.

- Done: Добавить persistent production state для scope `prototype`, `vertical_slice` и `full_game`.
- Done: Разбить производство на milestone Prototype, Vertical Slice, Alpha, Beta и Release.
- Done: Добавить workstreams для геймдизайна, инженерии, gameplay, level design, 2D/3D, анимации, аудио, UI/UX, интеграции, QA и релиза.
- Done: Реализовать dependency-driven задачи со статусами planned, ready, in progress, blocked и done.
- Done: Запретить завершение production-задачи без validation или существующего артефакта.
- Done: Добавить milestone gates: завершённые задачи, успешный gameplay playtest и production validation перед Release.
- Done: Добавить agent tools, строгие provider schemas, governance и production-контекст в системный prompt.
- Done: Добавить Game Production в Project Command Center с прогрессом, текущим потоком и быстрыми действиями.
- Done: Добавить production-планы, задачи, зависимости и артефакты в Project Map.
- Done: Добавить тесты, документацию и включить Stage 43 в общий production preflight.

## Этап 44 - Vertical Slice Orchestrator

Цель: провести один репрезентативный игровой срез через код, ассеты, Unreal-интеграцию, playtest и production gate как единый возобновляемый run.

- Done: Добавить persistent run state с фазами, зависимостями, evidence, артефактами и журналом переходов.
- Done: Построить dependency graph Preflight -> Gameplay/Visual Assets -> Level Integration -> Experience -> Playtest -> Production Gate.
- Done: Разрешить Gameplay Foundation и Visual Assets выполняться параллельно ограниченными субагентами при сохранении главного агента владельцем orchestration state.
- Done: Добавить live gates для Unreal project/engine/MCP, gameplay plan, import-ready assets, workspace artifacts, playtest и production milestone.
- Done: Добавить agent tools, строгие provider schemas, governance и Vertical Slice контекст в системный prompt.
- Done: Добавить Vertical Slice Orchestrator в Project Command Center с прогрессом, ready-фазами, recommended tools и быстрыми действиями.
- Done: Добавить run, фазы, зависимости и артефакты Vertical Slice в Project Map.
- Done: Добавить тесты фазового графа, документацию и включить Stage 44 в общий production preflight.

## Этап 45 - Project-Aware Game Task Constructor

Цель: превратить пользовательское намерение в подтверждённую игровую задачу, привязанную к точным узлам Project Map, а не к предположениям модели.

- Done: Добавить persistent-модуль `game_task_builder` с общей сессией для prompt и режима `Проект`.
- Done: Добавить каталог из 8 сфер, 10 направлений в каждой и 10 операций в каждом направлении: 800 встроенных конечных сценариев и отдельный `Свой вариант`.
- Done: Сделать первый глубокий Project Map scan обязательным для изменяющих Unreal-задач, а последующие обновления инкрементальными.
- Done: Добавить readiness-состояния, health-report, Asset Registry/MCP/Engine диагностику и guided remediation.
- Done: Расширить Project Map типами Skeleton, Skeletal/Static Mesh, Animation Blueprint/Montage, Control Rig, Physics Asset, Sound, UI и Input.
- Done: Расширить Project Map семантическими связями `uses_skeleton`, `animates`, `controlled_by`, `has_component`, `compatible_with`, `spawned_by`, `owned_by`, `bound_to_input`, `produces`, `consumes` с provenance, confidence и временем обнаружения.
- Done: Сохранять ручные связи после refresh, а связи агента проводить через proposal с отдельным принятием или отклонением.
- Done: Реализовать Target Resolver по актуальной карте и детерминированным контрактам; Static Mesh запрещён как цель персонажной анимации.
- Done: Реализовать диагностику `ready / needs_setup / ambiguous / stale_context / external_tool_required` и варианты подготовки с риском, временем, инструментами и approvals.
- Done: Добавить структурированную карточку понимания задачи, opt-in улучшения, ускорение/субагентов и действия подтверждения или изменения плана.
- Done: Создавать immutable `TaskManifest` с graph fingerprint, node IDs и object paths; проверять изменяющие tools и MCP `_meta` по подтверждённому scope.
- Done: Добавить agent tools, строгие provider schemas, governance, audit events и game-task контекст в системный prompt.
- Done: Добавить unit-тесты, Unreal fixture `BP_Hero / SK_Hero / ABP_Hero / SM_Bucket`, документацию и live checklist Unreal Engine 5.8.
- Done: Добавить встроенный Unreal Setup Wizard без терминала: выбор проекта, автообнаружение UE 5.8/toolchain/MCP, диагностика плагинов и запуск анализа одной кнопкой.
- Done: Добавить безопасное включение `PythonScriptPlugin`, `EditorScriptingUtilities` и опционального `ModelContextProtocol` через подтверждение, allowlist и резервную копию `.uproject`.
- Done: Оставить env-переменные и PowerShell-команды только для CI/developer live validation, а не для пользовательского сценария конструктора.
- Done: Добавить очевидный Unreal-onboarding на домашний экран: `Проект открыт -> Project Map -> Задача агенту`, одну главную кнопку старта, состояние анализа и переход к карте проекта.
- Done: Автоматически открывать мастер подготовки при первом выборе ещё не проанализированного Unreal-проекта; после готовности менять главное действие на `Создать задачу для игры`.
- Done: Ограничить игровой Unreal-pipeline проектами с реально обнаруженным `.uproject`; для Rust, Node/React, Python, Unity, Godot, standalone-плагинов и обычных папок сохранять их собственный общий сценарий.
- Done: Перенести конструктор из модального окна в постоянную центральную вкладку рядом с `Agent`, сохранив одну сессию и быстрые точки входа.
- Done: Убрать повторный readiness scan и пересборку каталога из каждого UI-кадра; кэшировать Project Map readiness и каталог из 800 сценариев.
- Done: Показывать каталог при частично построенной карте, объяснять ограничение изменяющих операций и выводить статус, длительность и результат последнего глубокого анализа.
- Done: Заменить технические метрики Project Map на читаемый developer-report: вывод анализа, четыре слоя готовности, состав найденных узлов, расшифровку unresolved/ambiguous и доступные действия.
- Done: Добавить прямые действия `Открыть Project Map`, `Повторить глубокий анализ`, ручной импорт Asset Registry и запуск Unreal Editor из результата анализа.
- Done: Исправить Windows-путь headless Python exporter и считать Unreal scan успешным только при фактическом создании нового Asset Registry snapshot.
- Done: Сделать путь к полной карте однокнопочным: `Завершить анализ автоматически`, а ручной JSON/Editor оставить запасной диагностикой.
- Done: Поддержать большие Asset Registry до 96 МБ, восстанавливать пустой `object_path` UE 5.8 и экспортировать проектный `/Game` с семантическими tags.
- Done: Не считать зависимости Engine/плагинов неразрешёнными целями проекта; readiness учитывает только отсутствующие ссылки внутри `/Game`.
- Done: Устранить чтение и повторное сканирование многомегабайтной Project Map в каждом кадре через workspace-scoped cache.
- Done: Добавить производительный обзор Project Map по смысловым группам, детальный режим, wheel zoom от курсора, pan, LOD связей и viewport culling.
- Done: Добавить постоянную вкладку `Проект` в правую рабочую панель: сохранённый итог анализа, готовность данных, состав карты, проблемные места, последний запуск и следующие действия; автоматически открывать её при старте и завершении анализа.
- Done: Заменить непрозрачный статус `устарела` на контроль реальных изменений: исключить служебные файлы Leetcode из fingerprint, показывать добавленные, изменённые и удалённые входы Unreal-проекта и предлагать синхронизацию карты.

## Этап 46 - Иерархическая Project Map

Цель: заменить нечитаемую матрицу узлов рабочим исследователем структуры, зависимостей и последствий изменений.

- Done: Разделить карту на четыре режима `Обзор`, `Структура`, `Зависимости` и `Влияние` с единым выбранным узлом.
- Done: Построить ленивое иерархическое дерево по связям `contains` и `declares`, ограничить число одновременно отображаемых ветвей и показывать количество скрытых потомков.
- Done: Добавить фокусную раскладку: входящие связи слева, выбранный узел в центре, исходящие связи справа; глубина обхода настраивается от одного до трёх уровней.
- Done: Добавить направленные стрелки, подсветку окружения узла и hover-подсказки для рёбер с понятным названием, техническим типом, источником и confidence.
- Done: Добавить режим цепочки влияния, поиск узла с переводом фокуса и сохранение выбранного узла как контекста агента/MCP.
- Done: Не отображать весь граф из тысяч элементов одновременно: применять lazy expansion, лимиты ветвей, кэшированные adjacency-индексы и раскладку только видимого подграфа.
- Done: Добавить unit-тесты режимов, структурной и направленной раскладки и подписей связей.
- Done: Заменить бесконечную структурную раскладку локальным браузером ветви: родитель, текущий контейнер, 12 дочерних узлов на страницу и предпросмотр следующего уровня.
- Done: Кэшировать `contains/declares`-индексы один раз при загрузке карты, чтобы навигация и pan/zoom не пересобирали десятки тысяч связей на каждом кадре.
- Done: Добавить общую историю фокуса, кнопки назад/вперёд/к родителю/к корню и кликабельные хлебные крошки во всех детальных режимах.
- Done: Отделить причинное влияние от обычных зависимостей: учитывать семантическое направление `depends_on`, `uses_skeleton`, `controlled_by`, `generates` и других связей, показывать прямые и косвенные последствия отдельными уровнями.

## Постоянная планка качества

- Every tool that mutates files, runs shell commands, calls paid APIs, or controls the desktop must have policy checks and visible audit logs.
- Every provider integration must include a minimal mocked test and a documented manual validation path.
- UI should make agent state visible: thinking/streaming, tool running, waiting for approval, cancelled, failed, or complete.
