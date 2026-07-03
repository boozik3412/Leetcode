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

It also creates a distributable archive and checksum by default:

```text
dist/leetcode-portable.zip
dist/leetcode-portable.sha256.txt
```

To skip archive creation:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipArchive
```

To sign the packaged executable before the archive is created, install Windows SignTool and run:

```powershell
$env:LEETCODE_SIGN_CERT_THUMBPRINT = "<certificate thumbprint>"
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -Sign
```

If `signtool.exe` is not on `PATH`, set `SIGNTOOL_PATH` or pass `-SignToolPath`.

## Release Checklist

- Build with `cargo +stable build --release`.
- Verify `run-leetcode.cmd check` and `run-leetcode.cmd test`.
- Open the app and check `Контроль -> Диагностика`: git/cargo/rustup, config path, journal path, proxy state, and workspace data path should look correct.
- Confirm `assets/app-icon.png` exists and matches the current brand.
- Open the packaged executable on a clean Windows user profile.
- Confirm saved API keys, proxy settings, project selection, and generated assets paths.
- Confirm logs and generated files stay inside the selected workspace or documented config/data folders.
- Confirm `dist/leetcode-portable.zip` and `dist/leetcode-portable.sha256.txt` are regenerated for the release.
- If distributing outside a private machine, sign the executable or archive with the project certificate.

## Diagnostics

- The runtime diagnostics panel lives under `Контроль -> Диагностика`.
- Config is stored in the OS config directory under `leetcode/config.json`.
- The durable journal is stored in the OS data directory under `leetcode/journal.log`.
- Panic crash reports are stored in the OS data directory under `leetcode/crashes`.
- Workspace-local generated state is stored under `assets/generated/leetcode`.
- Fatal crashes are intercepted with a panic hook that writes app version, process info, panic location, payload, and backtrace before the default panic handler runs.

## Still To Add

- Installer UX for non-technical users.
- Optional GPU/backend diagnostics.
- Optional native minidumps beyond Rust panic reports.
