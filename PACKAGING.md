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

## Thin Client Windows Build

Run from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-client-windows.ps1
```

The script builds only the remote desktop client binary:

```text
target/release/leetcode-client.exe
```

Then it creates:

```text
dist/leetcode-client-portable/
dist/leetcode-client-portable.zip
dist/leetcode-client-portable.sha256.txt
dist/client-latest.json
```

The client package folder and zip include:

```text
leetcode-client.exe
install-leetcode-client.ps1
uninstall-leetcode-client.ps1
package-info.json
README.md
PACKAGING.md
```

Install the client for the current Windows user:

```powershell
cd dist\leetcode-client-portable
powershell -ExecutionPolicy Bypass -File .\install-leetcode-client.ps1
```

Default install location:

```text
%LOCALAPPDATA%\Programs\Leetcode Client
```

The client installer creates `Leetcode Client` shortcuts in the Start Menu and on the Desktop. It keeps saved remote URL/token config in the OS config/data folders when uninstalled.

To uninstall the client binaries and shortcuts:

```powershell
powershell -ExecutionPolicy Bypass -File .\uninstall-leetcode-client.ps1
```

Current client distribution is separate from the main app update channel. Publish `client-latest.json`, `leetcode-client-portable.zip`, and `leetcode-client-portable.sha256.txt` when a separate client update channel is needed.

## Update Manifest Foundation

`dist/latest.json` is the first release-channel contract for a future updater and thin client. It contains app name, version, channel, platform, archive filename, SHA256, size, installer script name, uninstaller script name, and publish timestamp.

Current behavior:

- Manual distribution can publish `leetcode-portable.zip`, `leetcode-portable.sha256.txt`, and `latest.json` together.
- The in-app Release Cockpit can download `latest.json`, compare versions, download the zip, verify SHA256, stage the update, launch an external updater process, close Leetcode, copy the new files into the install folder, and restart Leetcode.
- Leetcode checks the configured `latest.json` once on startup and reports whether a newer version is available. The startup check never installs anything by itself.
- The default update channel is `https://github.com/boozik3412/Leetcode/releases/latest/download/latest.json`. Publish `latest.json`, `leetcode-portable.zip`, and `leetcode-portable.sha256.txt` as GitHub Release assets to use the default channel.
- Dev builds running from `target/debug` or `target/release` intentionally refuse self-update. Test updater behavior from an installed build under `%LOCALAPPDATA%\Programs\Leetcode`.
- Code signing, rollback UI, staged channels, and silent background updates are still planned work.

## In-App Update Button

Open `Project -> Release` inside Leetcode and use `Обновить и перезапустить` in the `Автообновление` block.

The button does this:

1. Reads the configured `Manifest URL`.
2. Downloads and validates `latest.json`.
3. Compares the manifest version with the running app version.
4. Downloads `leetcode-portable.zip`.
5. Verifies the archive SHA256 and optional size.
6. Writes a temporary updater script.
7. Starts that updater, exits the current app process, replaces files in the install directory, and starts Leetcode again.

The last updater log is written to the OS data directory:

```text
leetcode/updates/last-update.log
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
- Code signing, rollback UI, staged channels, and optional silent background checks.
- Optional GPU/backend diagnostics.
- Optional native minidumps beyond Rust panic reports.
