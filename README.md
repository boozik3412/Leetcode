# Leetcode

Minimal desktop coding agent implemented as a Rust desktop app.

## What works in this MVP

- One-window desktop UI with project picker, file tree, chat, tool log, and input field.
- Editable file buffer with `Save`, `Revert`, and `Reload`.
- OpenAI Responses API agent loop.
- Model provider layer with OpenAI, Claude, DeepSeek, and Gemini providers.
- Per-provider model/API key config with task route selection and provider fallback.
- Task-based model routing for coding, planning, fast, vision, image, audio, video, and realtime workflows.
- Image asset generation panel with ChatGPT Image, Nano Banana, Stability AI, and Replicate FLUX providers.
- Media asset jobs for spritesheets, OpenAI audio, and OpenAI/Sora video.
- Agent-callable `generate_image_asset` tool for creating visual game/app assets from chat requests.
- Agent-callable asset tools: `generate_spritesheet_asset`, `generate_audio_asset`, `generate_video_asset`, `regenerate_image_asset`, `vary_image_asset`, `upscale_asset`, `export_asset`, `attach_asset`, `use_asset_as_app_icon`, and `open_asset_folder`.
- Agent-callable `screenshot` tool for approved desktop screenshots saved into the selected workspace.
- Approval-gated desktop control tools: `active_window`, `focus_window`, `desktop_step`, `mouse_click`, `type_text`, and `hotkey`.
- Project profile detection for Rust, Node/package.json, React/Vite, Python, Godot, Unity, and Unreal workspaces.
- Project panel quick commands for detected run/check/test/build/dev/lint/editor workflows.
- Agent-callable `project_command` tool for common project lifecycle commands.
- Persistent terminal panel for long-running shells, REPLs, dev servers, and watchers.
- Game/app workflow templates for prototype mechanics, spritesheets, UI sounds, item icons, vertical slices, and playtest checklists.
- Browser/app preview hooks for Vite/React/Next/Trunk URLs and Godot/Unreal editor commands.
- Multi-agent orchestration layer with Code Agent, Game Designer, Art Director, Audio Agent, QA Agent, and Build Agent handoffs.
- Bounded subagent execution through `run_subagent`, with role-specific tool allowlists and traceable run records.
- Shared workspace context, run summaries, trace export, and replayable eval case files under `assets/generated/orchestration`.
- Project-local generated image outputs under `assets/generated/images`.
- Streaming model text updates from OpenAI Responses API server-sent events.
- Single exposed model tool: `act`.
- Local actions inside the selected workspace:
  - `list_files`
  - `read_file`
  - `write_file`
  - `edit_file`
  - `apply_patch`
  - `grep`
  - `project_command`
  - `game_workflow`
  - `open_project_preview`
  - `run_subagent`
  - `delegate_agent`
  - `update_workspace_context`
  - `record_run_summary`
  - `export_trace`
  - `create_replay_eval`
  - `orchestration_snapshot`
  - `run_shell`
  - `terminal_start`
  - `terminal_write`
  - `terminal_read`
  - `terminal_stop`
  - `terminal_clear`
  - `generate_image_asset`
  - `generate_spritesheet_asset`
  - `generate_audio_asset`
  - `generate_video_asset`
  - `upscale_asset`
  - `export_asset`
  - `attach_asset`
  - `use_asset_as_app_icon`
  - `open_asset_folder`
  - `governance_snapshot`
  - `set_tool_enabled`
  - `set_category_enabled`
  - `add_shell_deny_pattern`
  - `memory_snapshot`
  - `record_project_goal`
  - `upsert_task`
  - `update_task_status`
  - `record_decision`
  - `asset_library_snapshot`
  - `tag_asset`
  - `favorite_asset`
  - `export_asset_pack`
  - `run_replay_eval`
  - `eval_snapshot`
  - `provider_health_snapshot`
- Approval prompts for shell and write/edit actions.
- Patch dry-run validation with `git apply --check` before approval.
- Git status/diff summary in the tool panel.
- Runtime panel with agent/project/assets/terminal state, active model state, permission summary, and pending approval status.
- Durable action journal under the user data directory with a right-side Journal viewer.
- Saved permission modes in the prompt bar: Ask, Auto, Work, Full, plus backward-compatible Custom configs.
- Governance panel for disabling tools/categories and adding shell deny patterns.
- Project Memory panel with persistent goals, tasks, and decisions.
- Asset Library panel with generated asset indexing, tags, favorites, and pack export support.
- Evals panel for local static replay eval checks.
- Providers panel for offline model/API key health reporting.
- Basic cancellation flag for active agent runs.

## Setup

Install Rust, then run:

```powershell
cd C:\Users\Admin\Documents\Agent
$env:OPENAI_API_KEY = "your-openai-key"
cargo run
```

You can also paste an API key into the top bar at runtime and click `Save`. The key is stored in the current Windows user's Leetcode config file together with provider, model, workspace, policy profile, and approval settings, so it survives rebuilds and app restarts. Provider-specific environment variables still take priority over saved keys when they are set:

```powershell
$env:OPENAI_API_KEY = "your-openai-key"
$env:ANTHROPIC_API_KEY = "your-anthropic-key"
$env:DEEPSEEK_API_KEY = "your-deepseek-key"
$env:GEMINI_API_KEY = "your-gemini-key"
$env:STABILITY_API_KEY = "your-stability-key"
$env:REPLICATE_API_TOKEN = "your-replicate-token"
```

The provider selector currently supports:

- OpenAI through the Responses API.
- Claude through the Anthropic Messages API.
- DeepSeek through its OpenAI-compatible Chat Completions API.
- Gemini through the Google Gemini Generate Content API.

OpenAI streams text deltas progressively. Claude, DeepSeek, and Gemini currently return completed provider turns while still supporting the same local `act` tool loop. The `Route` selector can stay on `Auto` or force a task route such as Coding, Planning, Fast, Vision, Image, Audio, Video, or Realtime. When a routed provider/model fails before a tool step, Leetcode tries the next compatible saved-key fallback and shows a friendly provider error.

Asset generation has its own kind/provider controls in the `Assets` panel:

- ChatGPT Image through the OpenAI Images API, default model `gpt-image-2`.
- Nano Banana through the Gemini Interactions API, default model `gemini-3.1-flash-image`.
- Stability AI through Stable Image Core, default model label `stable-image-core`.
- Replicate FLUX through Replicate model predictions, default model `black-forest-labs/flux-schnell`.
- Spritesheets reuse image providers with a grid-oriented prompt.
- Audio uses OpenAI audio output, default model `gpt-audio-1.5`.
- Video uses the OpenAI Videos API/Sora, default model `sora-2`.

OpenAI and Gemini image generation can reuse the saved chat provider keys. Stability AI and Replicate use their own saved image keys or the environment variables above.

The coding agent can also call asset tools itself when a user asks for game/app visuals, spritesheets, sounds, or videos. Because this can call paid external APIs, the app asks for approval before the request is sent. Asset jobs include provider/model/parameter/license metadata in `assets/generated/asset_jobs.json`. Existing assets can be varied, upscaled, exported, attached as context, applied as `assets/app-icon.png`, or revealed in the file explorer. Screenshots are approval-gated and are saved under `assets/generated/screenshots`.

The prompt bar has a permission-mode switcher:

- `Запрос` asks before every mutating, shell, paid API, desktop, external, or orchestration action.
- `Авто` lets the agent edit the selected workspace and run routine commands, but still asks for paid API calls, desktop control, external opens, and destructive shell commands.
- `Работа` also allows paid asset/model API calls without prompting, while still asking for desktop control, external opens, and destructive shell commands.
- `Полный` removes approval prompts for all agent actions, while keeping workspace path checks and tool argument validation.

Desktop control currently supports active-window inspection, approval-gated screenshots, window focus, desktop steps, mouse clicks, typed text, and keyboard shortcuts. For desktop work, the preferred loop is `active_window` or `desktop_step` observe first, then one focused `desktop_step` action that captures before and after screenshots.

Project profiles are detected from common root markers such as `Cargo.toml`, `package.json`, `pyproject.toml`, `project.godot`, Unity `ProjectSettings`, and `.uproject` files. The right-side `Project` panel exposes safe quick commands from those profiles, and the agent is instructed to prefer `project_command` for common check/test/run/build/dev/lint tasks before falling back to raw `run_shell`.

The `Terminal` panel owns one persistent shell session shared by the UI and agent tools. Use it for dev servers, REPLs, watchers, game engine logs, and commands where the shell state should persist across turns. The agent can call `terminal_start`, `terminal_write`, `terminal_read`, `terminal_stop`, and `terminal_clear`; writes and stops are approval-gated when shell approval is enabled.

The `Project` panel also exposes game workflow templates. They create markdown plans under `docs/game-workflows` and can be called by the agent through `game_workflow`. Preview hooks can open common local URLs such as Vite `5173`, Vite preview `4173`, Next `3000`, Trunk `8080`, or return the editor command to run for Godot/Unreal through `open_project_preview`.

The `Agents` panel exposes the Rust-owned orchestration layer. It can record specialist handoffs from the current prompt, show a workspace orchestration snapshot in the tool log, and export a trace JSON file. The agent can call the same orchestration tools itself. `run_subagent` lets the manager agent execute a bounded specialist mini-loop for a focused task; subagents have role-specific tool allowlists, max-round limits, and their runs are saved in the orchestration trace. For broad tasks, the manager is instructed to propose a compact subagent plan first unless the user has already approved splitting the work. The current architecture keeps orchestration inside the Rust desktop app for a self-contained MVP; an OpenAI Agents SDK sidecar remains the planned upgrade path when independent specialist execution, hosted tracing, or richer session management becomes necessary.

The `Runtime` panel shows whether the main agent, project command, asset job, or terminal is running, plus the effective permission mode and current provider/model state. The `Journal` panel shows the latest durable audit entries and can refresh or clear the local journal file.

The `Governance` panel stores workspace-level tool rules under `assets/generated/leetcode/governance.json`. It can disable individual tools, disable whole categories, and add shell deny patterns that block matching `run_shell`, `terminal_write`, or `project_command` calls before execution.

The `Memory` panel stores project goals, tasks, and decisions in `assets/generated/leetcode/memory.json`. The agent receives a compact memory summary at the start of each turn and can update memory through `memory_snapshot`, `record_project_goal`, `upsert_task`, `update_task_status`, and `record_decision`.

Conversations are persisted per workspace under `assets/generated/leetcode/conversations`. Each chat is stored as JSONL plus a small state file, so the active dialogue, elapsed-response metadata, provider response state, and pinned per-chat notes can be restored after restarting the app. The chat header can create, switch, rename, pin, archive, restore, or delete saved conversations. Before each agent turn, Leetcode compiles a compact context block from pinned notes, the rolling transcript summary, recent messages, relevant older messages, and recent structured run history; the context inspector shows that block before the request is sent. It also includes quick presets for short/balanced/deep context, manual budget controls for recent messages, retrieved older messages, and run-history items, plus context profile export/import through JSON files stored under `assets/generated/leetcode/context_profiles`. After longer or broader successful runs, Leetcode suggests new pinned notes from the confirmed plan, changed files, and final response; the user can save one, save all, or dismiss them before they become durable context. Provider-side `previous_response_id` is still reused when possible, but local transcript context is the durable fallback.

The `Asset Library` panel indexes completed generated asset jobs into `assets/generated/leetcode/asset_library.json`. It supports filtering, favorites, tags through the agent, and exporting selected/favorite/tagged assets into an asset pack folder.

The `Evals` panel runs local static checks for replay eval cases created by `create_replay_eval` and stores results in `assets/generated/leetcode/eval_results.json`. These checks validate prompt presence, expected tool names, and success criteria without calling external APIs.

The `Providers` panel reports offline health for configured chat and asset providers: API key presence, selected model, registry coverage, capabilities, and issues. Manual live smoke checks validate a real text response and tool-call shape for chat providers, while paid asset smoke checks launch small Asset Studio jobs only after a user click. Provider validation history is stored per workspace in `assets/generated/leetcode/provider_validation_results.json`. The agent can request the same offline report with `provider_health_snapshot`.

## Launch

On Windows, double-click `run-leetcode.cmd` or run it from PowerShell:

```powershell
.\run-leetcode.cmd
```

The launcher uses the local `.cargo` and `.rustup` toolchain folders when they exist. It also supports quick development commands:

```powershell
.\run-leetcode.cmd check
.\run-leetcode.cmd test
.\run-leetcode.cmd build
.\run-leetcode.cmd release
```

## Packaging

Portable Windows packaging is documented in `PACKAGING.md`:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

The output is written to `dist/leetcode-portable`.

The package script also creates `dist/leetcode-portable.zip` and `dist/leetcode-portable.sha256.txt` by default. Pass `-Sign` with `LEETCODE_SIGN_CERT_THUMBPRINT` when a Windows code-signing certificate is available.

Crash reports from Rust panics are written to the OS data directory under `leetcode/crashes` and are shown in the diagnostics panel.

Before packaging, open `Контроль -> Диагностика` inside the app. It reports the executable path, config path, journal path, selected workspace data path, proxy state, and local toolchain availability without exposing API keys. The agent can request the same read-only report with `environment_snapshot`.

## Notes

Desktop actions operate on the currently active Windows desktop and are intentionally approval-gated. Use `desktop_step` when asking the agent to interact with UI coordinates because it captures the screen before and after each action. The right-side `Desktop` panel shows the latest captured screenshot and active-window summary.

`run_shell` uses Windows PowerShell by default and writes commands to a temporary `.ps1` file before execution. This is more reliable for multi-line commands and nested quotes than `cmd /C`. Agents can still request `shell: "cmd"` when a command specifically needs `cmd.exe`.

For interactive or long-running work, prefer the persistent terminal tools over `run_shell`.

## Validation

```powershell
cargo fmt
cargo check
cargo test
.\run-leetcode.cmd check
```
