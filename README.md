# Leetcode

Minimal desktop coding agent implemented as a Rust desktop app.

## What works in this MVP

- One-window desktop UI with project picker, file tree, chat, tool log, and input field.
- Editable file buffer with `Save`, `Revert`, and `Reload`.
- OpenAI Responses API agent loop.
- Model provider layer with OpenAI, Claude, DeepSeek, and Gemini providers.
- Per-provider model/API key config foundation.
- Image asset generation panel with ChatGPT Image, Nano Banana, Stability AI, and Replicate FLUX providers.
- Agent-callable `generate_image_asset` tool for creating visual game/app assets from chat requests.
- Agent-callable `screenshot` tool for approved desktop screenshots saved into the selected workspace.
- Approval-gated desktop control tools: `mouse_click`, `type_text`, and `hotkey`.
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
  - `run_shell`
- Approval prompts for shell and write/edit actions.
- Patch dry-run validation with `git apply --check` before approval.
- Git status/diff summary in the tool panel.
- Durable action journal under the user data directory.
- Basic cancellation flag for active agent runs.

## Setup

Install Rust, then run:

```powershell
cd C:\Users\Admin\Documents\Agent
$env:OPENAI_API_KEY = "your-openai-key"
cargo run
```

You can also paste an API key into the top bar at runtime and click `Save`. The key is stored in the current Windows user's Leetcode config file together with provider, model, workspace, and approval settings, so it survives rebuilds and app restarts. Provider-specific environment variables still take priority over saved keys when they are set:

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

OpenAI streams text deltas progressively. Claude, DeepSeek, and Gemini currently return completed provider turns while still supporting the same local `act` tool loop.

Image generation has its own provider selector in the `Assets` panel:

- ChatGPT Image through the OpenAI Images API, default model `gpt-image-2`.
- Nano Banana through the Gemini Interactions API, default model `gemini-3.1-flash-image`.
- Stability AI through Stable Image Core, default model label `stable-image-core`.
- Replicate FLUX through Replicate model predictions, default model `black-forest-labs/flux-schnell`.

OpenAI and Gemini image generation can reuse the saved chat provider keys. Stability AI and Replicate use their own saved image keys or the environment variables above.

The coding agent can also call `generate_image_asset` itself when a user asks for a visual asset. Because this can call paid external APIs, the app asks for approval before the request is sent. Screenshots are also approval-gated and are saved under `assets/generated/screenshots`.

Desktop control currently supports approval-gated screenshots, mouse clicks, typed text, and keyboard shortcuts. For desktop work, the intended loop is screenshot first, then act on visible coordinates or the active window.

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

## Notes

Desktop actions operate on the currently active Windows desktop and are intentionally approval-gated. Use screenshots first when asking the agent to interact with UI coordinates.

`run_shell` uses Windows PowerShell by default and writes commands to a temporary `.ps1` file before execution. This is more reliable for multi-line commands and nested quotes than `cmd /C`. Agents can still request `shell: "cmd"` when a command specifically needs `cmd.exe`.

## Validation

```powershell
cargo fmt
cargo check
cargo test
.\run-leetcode.cmd check
```
