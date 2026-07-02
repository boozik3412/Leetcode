# Leetcode

Minimal desktop coding agent implemented as a Rust desktop app.

## What works in this MVP

- One-window desktop UI with project picker, file tree, chat, tool log, and input field.
- Editable file buffer with `Save`, `Revert`, and `Reload`.
- OpenAI Responses API agent loop.
- Streaming model text updates from Responses API server-sent events.
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
$env:OPENAI_API_KEY = "your-key"
cargo run
```

You can also paste an API key into the top bar at runtime. The key is not saved to disk; model, workspace, and approval settings are saved under the user config directory.

## Notes

The `screenshot` action is intentionally present as a reserved tool but is not implemented in this first version. It gives the next desktop-control phase a stable place to add screen capture, click, typing, hotkeys, and window focus.

`run_shell` uses Windows PowerShell by default and writes commands to a temporary `.ps1` file before execution. This is more reliable for multi-line commands and nested quotes than `cmd /C`. Agents can still request `shell: "cmd"` when a command specifically needs `cmd.exe`.

## Validation

```powershell
cargo fmt
cargo check
cargo test
```
