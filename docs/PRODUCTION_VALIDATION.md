# Production Validation

Stage 42 объединяет локальные тесты, runtime smoke, release manifests, Unreal-проверки и визуальные эталоны в один воспроизводимый контур перед публикацией Leetcode или игрового проекта.

## Режимы preflight

Запускайте из корня репозитория:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/production-preflight.ps1 -Mode Quick
powershell -ExecutionPolicy Bypass -File scripts/production-preflight.ps1 -Mode Release
powershell -ExecutionPolicy Bypass -File scripts/production-preflight.ps1 -Mode Live
```

- `Quick`: `cargo fmt --check`, `cargo check --all-targets`, тесты, debug-сборка host/client/relay, `--production-smoke` каждого бинарника и реальный `GET /health` временного relay.
- `Release`: всё из Quick в release-профиле, упаковка host/client и проверка SHA256/размера `latest.json` и `client-latest.json`.
- `Live`: всё из Release плюс явно включённые контракты провайдеров, MCP и установленного Unreal Engine.

Итог каждого прохода сохраняется в `assets/generated/leetcode/production-validation/preflight-run.json`. В приложении тот же сценарий доступен через `Проект -> Релиз -> Production preflight`.

## Live contracts

Live-проверки никогда не стартуют неявно. Они могут использовать платные API, внешние процессы и реальный Unreal-проект.

```powershell
$env:LEETCODE_LIVE_PROVIDER_CONTRACTS = "1"
$env:LEETCODE_LIVE_PROVIDER_IDS = "openai,anthropic,gemini,deepseek" # необязательно

$env:LEETCODE_LIVE_MCP_CONTRACTS = "1"
$env:LEETCODE_LIVE_MCP_WORKSPACE = "C:\Projects\MyGame"
$env:LEETCODE_LIVE_MCP_IDS = "unreal-mcp" # необязательно

$env:LEETCODE_UE_VALIDATION_WORKSPACE = "C:\Projects\MyGame"
$env:UE_ENGINE_ROOT = "C:\Program Files\Epic Games\UE_5.8"
```

Провайдеры используют сохранённые настройки Leetcode или соответствующие переменные API-ключей. MCP-проверка запускает только включённые записи реестра выбранного workspace. Unreal-проход выполняет Automation tests и cook; в Live preflight также запрашивается package.

Unreal-проход можно запустить отдельно:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/validate-unreal-production.ps1 `
  -Workspace C:\Projects\MyGame `
  -TestFilter Project `
  -IncludePackage
```

Отчёт сохраняется в игровом проекте: `assets/generated/leetcode/unreal/production-validation.json`.

## Production report

Инструмент агента `production_validation_snapshot` и кнопка `Обновить отчёт` в Release Cockpit собирают `assets/generated/leetcode/production-validation/latest.json`. Отчёт проверяет:

- наличие release/preflight/Unreal-скриптов;
- host/client update manifests, package SHA256 и размер;
- историю live-проверок настроенных AI-провайдеров;
- состояние включённых MCP-серверов;
- Unreal 5.8, `UnrealEditor-Cmd.exe` и `RunUAT.bat`;
- структурный golden Project Map;
- visual baselines и последние сравнения;
- release-артефакты host/client/relay.

`warning` означает, что production-контур ещё неполон, но локальная функция не сломана. `failed` означает нарушенный контракт или несовместимый артефакт. `skipped` допустим для неиспользуемых внешних подсистем.

## Golden Project Map

`update_project_map_golden` сохраняет структурный хеш Project Map в `assets/generated/leetcode/production-validation/project-map.golden.json`. В хеш входят типы и ID узлов и связей, но не временные метки и UI-подписи.

Обновляйте golden только после ручной проверки осознанного архитектурного изменения. Не принимайте новый golden только ради зелёного отчёта.

## Visual regression

Поддерживаются сценарии:

- `desktop_main`
- `desktop_context`
- `desktop_roadmap`
- `desktop_release`
- `remote_client`
- `remote_pwa`

Агент использует `record_visual_baseline` и `compare_visual_snapshot` с workspace-relative PNG-путём. В Release Cockpit кнопки `Принять UI-эталон` и `Сравнить UI` работают с последним desktop screenshot и сценарием `desktop_release`.

Состояние хранится в `assets/generated/leetcode/visual-regression/state.json`, а PNG-эталоны — в `assets/generated/leetcode/visual-regression/baselines/`. Сравнение требует одинаковый размер, mean channel error не выше `0.025` и не более `10%` изменённых пикселей. Эталон всегда принимается вручную после визуального просмотра.

## Локальный release checklist

1. Запустить `-Mode Quick` во время разработки.
2. Обновить production report и устранить `failed`/`warning`, относящиеся к используемым подсистемам.
3. Сравнить desktop/client/PWA screenshots с принятыми эталонами.
4. Для Unreal-проекта пройти Automation, cook и package на установленном UE 5.8.
5. Запустить `-Mode Release`; проверить `dist/latest.json` и `dist/client-latest.json`.
6. Live-режим включать только с тестовыми ключами/лимитами и подготовленным проектом.
