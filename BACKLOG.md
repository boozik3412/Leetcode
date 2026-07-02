# Leetcode Development Backlog

Leetcode is evolving from a minimal local coding assistant into a desktop AI studio for game and app development. The product should support multiple AI providers, coding workflows, generated assets, project-aware tooling, and eventually multi-agent orchestration.

## Stage 1 - Reliable Coding Runtime

- Stream Responses API events into the UI instead of waiting for full model turns.
- Tighten tool schemas and validation so model arguments fail clearly and safely.
- Add dry-run patch validation with `git apply --check` before applying patches.
- Show patch impact and git status/diff in the UI.
- Add a durable action journal for agent runs, approvals, shell commands, patch attempts, and failures.
- Add focused tests for workspace path safety, tool argument parsing, patch validation, and shell command construction.
- Add a persistent terminal/PTY after the streaming and patch path is stable.

## Stage 2 - Model Provider Layer

- Done: Replace the direct `OpenAiClient` dependency in the runtime with a provider trait.
- Done: Add a model registry with capabilities: code, reasoning, tools, vision, image, audio, video, realtime, embeddings.
- Done: Keep OpenAI Responses as the default coding/reasoning provider.
- Done: Add provider settings and API key storage per provider.
- Done: Add implemented Claude, DeepSeek, and Gemini coding-agent providers.
- Add task-based model routing: coding, planning, cheap/fast, vision, image generation, audio generation, video generation.
- Add fallback models and clear provider error reporting.

## Stage 3 - Asset Generation Pipeline

- Done: Introduce asynchronous image asset jobs with status, provider, model, prompt, parameters, output files, and metadata.
- Done: Add image asset previews in the tool panel.
- Done: Save generated image outputs into `assets/generated/images`.
- Done: Add image providers: ChatGPT Image, Nano Banana, Stability AI, and Replicate FLUX.
- Done: Expose image generation as the agent-callable `generate_image_asset` action.
- Done: Add asset cards with regenerate, variation, use-as-icon, open-folder, and load-prompt actions.
- Done: Add agent-callable asset follow-up actions: `regenerate_image_asset`, `vary_image_asset`, `use_asset_as_app_icon`, and `open_asset_folder`.
- Track licensing/provider metadata for generated assets.
- Add upscale, export, and attach-to-chat workflows.
- Extend the same job framework to spritesheets, audio, video, and generated UI/game assets.

## Stage 4 - Game/App Project Profiles

- Detect common project types: Rust, web, React/Vite, Godot, Unity, Unreal, Python, and mixed repos.
- Add project commands for run, test, build, lint, package, and preview.
- Add game-specific flows: prototype mechanic, generate spritesheet, generate UI sounds, create item icons, build vertical slice, run playtest checklist.
- Add browser/app preview hooks for web and game tooling where available.

## Stage 5 - Multi-Agent Orchestration

- Add specialist roles: Code Agent, Game Designer, Art Director, Audio Agent, QA Agent, Build Agent.
- Add handoff records and shared workspace context.
- Evaluate whether to keep the current Rust-owned orchestration or add an OpenAI Agents SDK sidecar for richer tracing, sessions, and handoffs.
- Add run summaries, trace export, and replayable evaluations.

## Stage 6 - Desktop and Computer Control

- Add screenshot capture, click, type, hotkey, and window focus tools.
- Done: Add approval-gated screenshot capture into `assets/generated/screenshots`.
- Done: Add approval-gated mouse click, text typing, and hotkey desktop tools.
- Follow the computer-use loop: screenshot, model action, execute, screenshot output.
- Keep allowlists and human approval for irreversible desktop actions.

## Ongoing Quality Bar

- Every tool that mutates files, runs shell commands, calls paid APIs, or controls the desktop must have policy checks and visible audit logs.
- Every provider integration must include a minimal mocked test and a documented manual validation path.
- UI should make agent state visible: thinking/streaming, tool running, waiting for approval, cancelled, failed, or complete.
