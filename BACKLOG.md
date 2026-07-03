# Leetcode Development Backlog

Leetcode is evolving from a minimal local coding assistant into a desktop AI studio for game and app development. The product should support multiple AI providers, coding workflows, generated assets, project-aware tooling, and eventually multi-agent orchestration.

## Stage 1 - Reliable Coding Runtime

- Done: Stream Responses API events into the UI instead of waiting for full model turns.
- Done: Tighten tool schemas and validation so model arguments fail clearly and safely.
- Done: Add dry-run patch validation with `git apply --check` before applying patches.
- Done: Show patch impact and git status/diff in the UI.
- Done: Add a durable action journal for agent runs, approvals, shell commands, patch attempts, and failures.
- Done: Add Journal and Runtime UI panels for state visibility and audit review.
- Done: Add saved Safe/Normal/Strict/Custom policy profiles over shell/write approvals.
- Done: Add focused tests for workspace path safety, tool argument parsing, patch validation, and shell command construction.
- Done: Add a persistent terminal session with start/write/read/stop tools, UI panel, and shared state across agent turns.

## Stage 2 - Model Provider Layer

- Done: Replace the direct `OpenAiClient` dependency in the runtime with a provider trait.
- Done: Add a model registry with capabilities: code, reasoning, tools, vision, image, audio, video, realtime, embeddings.
- Done: Keep OpenAI Responses as the default coding/reasoning provider.
- Done: Add provider settings and API key storage per provider.
- Done: Add implemented Claude, DeepSeek, and Gemini coding-agent providers.
- Done: Add task-based model routing: coding, planning, cheap/fast, vision, image generation, audio generation, video generation, and realtime.
- Done: Add fallback model/provider chains and clear provider error reporting.

## Stage 3 - Asset Generation Pipeline

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

## Stage 4 - Game/App Project Profiles

- Done: Detect common project types: Rust, Node/package.json, React/Vite, Python, Godot, Unity, Unreal, and mixed repos.
- Done: Add project commands for run, test, build, lint, package script, editor, preview, and release where the project exposes them.
- Done: Add a Project panel with quick commands and streamed output through the existing tool log.
- Done: Expose quick commands as the agent-callable `project_command` action.
- Done: Add game-specific flows: prototype mechanic, generate spritesheet, generate UI sounds, create item icons, build vertical slice, run playtest checklist.
- Done: Add browser/app preview hooks for web and game tooling where available.

## Stage 5 - Multi-Agent Orchestration

- Done: Add specialist roles: Code Agent, Game Designer, Art Director, Audio Agent, QA Agent, Build Agent.
- Done: Add handoff records and shared workspace context under `assets/generated/orchestration`.
- Done: Keep Rust-owned orchestration for the desktop MVP and document the future OpenAI Agents SDK sidecar path for richer hosted tracing, sessions, and independent specialist execution.
- Done: Add run summaries, trace export, replayable evaluation case files, and an `Agents` UI panel.
- Done: Expose orchestration as agent-callable tools: `run_subagent`, `delegate_agent`, `update_workspace_context`, `record_run_summary`, `export_trace`, `create_replay_eval`, and `orchestration_snapshot`.
- Done: Add bounded subagent execution with role-specific tool allowlists, max-round limits, approval-gated tool execution, and traceable `SubagentRun` records.
- Done: Instruct the manager agent to propose a subagent split for broad work before executing it, unless the user already approved using subagents.

## Stage 6 - Desktop and Computer Control

- Done: Add screenshot capture, click, type, hotkey, active-window inspection, and window focus tools.
- Done: Add approval-gated screenshot capture into `assets/generated/screenshots`.
- Done: Add approval-gated mouse click, text typing, and hotkey desktop tools.
- Done: Add `desktop_step` for the computer-use loop: screenshot, model action, execute, screenshot output.
- Done: Add a `Desktop` panel that shows the latest desktop step screenshot and active-window summary.
- Keep allowlists and human approval for irreversible desktop actions.

## Stage 7 - Permission Control UX

- Done: Add prompt-bar permission modes: Ask, Auto, Work, and Full.
- Done: Apply the selected mode across shell, file writes, paid asset APIs, desktop control, external opens, and orchestration tools.
- Done: Preserve backward compatibility for previous Safe/Normal/Strict/Custom saved configs.
- Done: Show the effective permission matrix in the Runtime panel.

## Stage 8 - Tool Governance Center

- Done: Add a tool registry with category, risk, and description metadata.
- Done: Add workspace-level governance config for disabled tools, disabled categories, and shell deny patterns.
- Done: Enforce governance before every agent tool execution.
- Done: Add a Governance UI panel and agent-callable governance tools.

## Stage 9 - Project Memory / Task Board

- Done: Add persistent project memory under `assets/generated/leetcode/memory.json`.
- Done: Add goals, tasks, decisions, and prompt-time memory summaries.
- Done: Add a Memory UI panel for quick goal/task/decision capture.
- Done: Expose memory snapshot and update tools to the agent.
- Done: Add project memory sources for imported files and saved notes, including agent-callable source capture.

## Stage 10 - Asset Library Manager

- Done: Add an asset library index over generated asset jobs.
- Done: Support tags, favorites, notes, source job metadata, and export packs.
- Done: Add an Asset Library UI panel with filtering and favorite toggles.
- Done: Expose asset library snapshot/tag/favorite/export tools to the agent.

## Stage 11 - Validation & Evals

- Done: Add static replay eval runner over existing orchestration eval cases.
- Done: Persist eval run results under `assets/generated/leetcode/eval_results.json`.
- Done: Add an Evals UI panel and agent-callable eval snapshot/run tools.

## Stage 12 - Model/Provider Health Panel

- Done: Add offline provider health reporting for chat and asset providers.
- Done: Check configured keys, selected models, registry coverage, and provider issues.
- Done: Add a Providers UI panel and agent-callable provider health snapshot.

## Stage 13 - UX Polish "Codex-style"

- Done: Add clear interface modes: Chat, Code, Assets, and Project.
- Done: Scope the right sidebar by the selected mode so it does not show every tool group at once.
- Done: Keep file tabs and agent chat as first-class central surfaces.
- Todo: Continue visual simplification after real screenshots from daily usage.

## Stage 14 - Project Command Center

- Done: Promote project profiles, quick commands, preview hooks, terminal state, and recent command output into a command-center view.
- Done: Add first-pass rerun and "ask agent to fix" actions for project command failures.
- Done: Add command-run records with status, timestamp, exit code, command, and compact stdout/stderr tail.
- Done: Add "fix", "rerun", and "open preview" actions tied to concrete command runs.
- Done: Build fix prompts from the exact failed command output so the agent receives actionable diagnostics.
- Done: Add lightweight build-error grouping for failed command output.
- Done: Add structured diagnostics by compiler/test framework with file, line, column, type, targeted fix prompts, and fix-request history.
- Todo: Add richer task tree across project milestones and launch/game-development workstreams.

## Stage 15 - Asset Studio

- Done: Promote asset generation and asset history into a dedicated workspace mode.
- Done: Show generated asset history, variants, favorites, export, attach, and source metadata from one place.
- Todo: Add side-by-side comparison for multiple generated results.
- Todo: Add project import targets for common game/app asset folders.

## Stage 16 - Real Provider Validation

- Done: Add a validation checklist for OpenAI, Claude, DeepSeek, Gemini, image, audio, and video providers.
- Done: Add safe manual live validation for chat providers through the real provider layer: model text response plus tool-call shape check.
- Done: Run provider validation through the configured HTTP/proxy client.
- Done: Add explicit paid smoke buttons for image, audio, and video providers that launch small Asset Studio jobs only after a user click.
- Done: Keep recent provider validation results visible in the Providers panel.
- Done: Persist provider validation results under `assets/generated/leetcode/provider_validation_results.json`.

## Stage 17 - Packaging

- Done: Add Windows packaging docs and a portable build script.
- Done: Add runtime environment diagnostics for executable/config/journal/workspace paths, proxy state, and local toolchain availability.
- Done: Expose environment diagnostics as the read-only `environment_snapshot` agent tool.
- Done: Add release checklist for icon, version, README, provider setup, diagnostics, and workspace/config storage.
- Done: Document current crash/log folder policy.
- Todo: Add installer build path or signed portable archive.
- Todo: Add optional crash dump folder and panic hook.

## Ongoing Quality Bar

- Every tool that mutates files, runs shell commands, calls paid APIs, or controls the desktop must have policy checks and visible audit logs.
- Every provider integration must include a minimal mocked test and a documented manual validation path.
- UI should make agent state visible: thinking/streaming, tool running, waiting for approval, cancelled, failed, or complete.
