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
dist/latest.json
```

The package folder and zip include:

```text
leetcode.exe
install-leetcode.ps1
uninstall-leetcode.ps1
package-info.json
assets/
README.md
BACKLOG.md
PACKAGING.md
run-leetcode.cmd
```

To skip archive creation:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipArchive
```

## User-Local Installer Foundation

After building or unpacking `dist/leetcode-portable.zip`, install Leetcode for the current Windows user:

```powershell
cd dist\leetcode-portable
powershell -ExecutionPolicy Bypass -File .\install-leetcode.ps1
```

Default install location:

```text
%LOCALAPPDATA%\Programs\Leetcode
```

The installer copies the portable app, writes `install.json`, and creates Start Menu plus Desktop shortcuts. It refuses install paths outside `%LOCALAPPDATA%\Programs` or `%ProgramFiles%`.

To install without a Desktop shortcut:

```powershell
powershell -ExecutionPolicy Bypass -File .\install-leetcode.ps1 -NoDesktopShortcut
```

For smoke tests or managed deployments, install without any shortcuts:

```powershell
powershell -ExecutionPolicy Bypass -File .\install-leetcode.ps1 -NoShortcuts
```

To uninstall the app binaries and shortcuts:

```powershell
powershell -ExecutionPolicy Bypass -File .\uninstall-leetcode.ps1
```

User data, saved API keys, conversations, project memory, and logs are intentionally kept in the OS config/data folders. Remove those manually only when you really want to reset the agent.

## Update Manifest Foundation

`dist/latest.json` is the first release-channel contract for a future updater and thin client. It contains app name, version, channel, platform, archive filename, SHA256, size, installer script name, uninstaller script name, and publish timestamp.

Current behavior:

- Manual distribution can publish `leetcode-portable.zip`, `leetcode-portable.sha256.txt`, and `latest.json` together.
- A future updater can download `latest.json`, compare versions, download the zip, verify SHA256, and run the installer.
- This is not silent auto-update yet. Code signing, rollback, staged channels, and in-app update UX are still planned work.

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
- Confirm `dist/latest.json` points to the archive and contains the matching SHA256.
- Unpack the archive and run `install-leetcode.ps1` on a clean Windows user profile.
- If distributing outside a private machine, sign the executable or archive with the project certificate.

## Diagnostics

- The runtime diagnostics panel lives under `Контроль -> Диагностика`.
- Config is stored in the OS config directory under `leetcode/config.json`.
- The durable journal is stored in the OS data directory under `leetcode/journal.log`.
- Panic crash reports are stored in the OS data directory under `leetcode/crashes`.
- Workspace-local generated state is stored under `assets/generated/leetcode`.
- Fatal crashes are intercepted with a panic hook that writes app version, process info, panic location, payload, and backtrace before the default panic handler runs.
- Installed app binaries live under `%LOCALAPPDATA%\Programs\Leetcode` by default; user data remains in the OS config/data folders.

## Still To Add

- Installer UI for non-technical users.
- In-app update checker and SHA256-verified update installer.
- Code signing and rollback.
- Optional GPU/backend diagnostics.
- Optional native minidumps beyond Rust panic reports.
