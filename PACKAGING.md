# Leetcode Packaging

This document tracks the desktop release path for Leetcode.

## Portable Windows Build

Run from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

The script builds `leetcode.exe` in release mode and copies it into:

```text
dist/leetcode-portable/
```

## Release Checklist

- Build with `cargo +stable build --release`.
- Verify `run-leetcode.cmd check` and `run-leetcode.cmd test`.
- Open the app and check `Контроль -> Диагностика`: git/cargo/rustup, config path, journal path, proxy state, and workspace data path should look correct.
- Confirm `assets/app-icon.png` exists and matches the current brand.
- Open the packaged executable on a clean Windows user profile.
- Confirm saved API keys, proxy settings, project selection, and generated assets paths.
- Confirm logs and generated files stay inside the selected workspace or documented config/data folders.
- Add installer packaging after the portable build path is stable.

## Diagnostics

- The runtime diagnostics panel lives under `Контроль -> Диагностика`.
- Config is stored in the OS config directory under `leetcode/config.json`.
- The durable journal is stored in the OS data directory under `leetcode/journal.log`.
- Workspace-local generated state is stored under `assets/generated/leetcode`.
- Fatal crash interception is not implemented yet; use the journal and workspace-local generated state for audit/debug context.

## Still To Add

- Installer or signed portable archive.
- Optional GPU/backend diagnostics.
- Optional crash dump folder and panic hook.
