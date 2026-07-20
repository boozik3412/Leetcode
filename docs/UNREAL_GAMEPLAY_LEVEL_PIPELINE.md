# Unreal Gameplay и Level Pipeline

Stage 41 добавляет воспроизводимый цикл `контекст -> gameplay-план -> изменение уровня -> playtest -> артефакты -> Project Map`. Агент не исполняет сгенерированный Python: Rust проверяет ограниченный декларативный manifest, после чего фиксированный скрипт `scripts/unreal/apply_gameplay_manifest.py` применяет его через Unreal Python API.

## Инструменты агента

- `gameplay_snapshot` — читает планы, последние запуски, выбранный узел Project Map и доступные recipes.
- `create_gameplay_plan` — создаёт Markdown-план в `docs/gameplay`, связывает его с картой, задачами, roadmap и выбранным узлом.
- `apply_gameplay_plan` — после write/shell approval запускает `UnrealEditor-Cmd` и применяет manifest.
- `run_gameplay_playtest` — запускает Automation, map smoke или Movie Render Queue и сохраняет итог с длительностью, issues и артефактами.

Recipes покрывают базовый уровень, third-person loop, взаимодействие, предметы/инвентарь, checkpoints, столкновение с AI, PCG-окружение, Niagara feedback, Enhanced Input и HUD/UMG. Сложные Blueprint, PCG, Niagara, Enhanced Input и UMG-графы остаются за обнаруженными инструментами Unreal MCP; агент обязан сначала вызвать `mcp_discover` и не угадывать имена операций.

## Декларативный manifest

Поддержаны операции:

- `load_level`, `create_level`, `save_level`;
- `spawn_actor`, `delete_actor`, `set_actor_transform`;
- `add_actor_component` с class path и относительным transform;
- `set_actor_property` только для `actor_label`, `hidden`, `can_be_damaged`, `tags`, `folder_path`;
- `create_data_asset` с `/Game/...` package path и `/Script/...` либо `/Game/...` class path.

Все пути, имена, значения, число операций и Automation filter проверяются на Rust-стороне. Скрипт не использует `eval`/`exec`, не принимает произвольный Python и пишет структурированный result JSON.

## Playtest и визуальные доказательства

Режимы `run_gameplay_playtest`:

1. `automation` запускает `Automation RunTests`, ожидает пустую очередь и экспортирует отчёт.
2. `map_smoke` загружает карту в `-game`, рендерит короткий 10-секундный benchmark и при необходимости делает screenshot.
3. `movie_render` запускает Movie Render Queue для существующих `LevelSequence` и `MoviePipelineConfig` assets. Это путь для воспроизводимой видеозаписи; конфигурация MRQ определяет формат и output directory.

Leetcode индексирует свежие файлы из `Saved/AutomationReports`, `Saved/Screenshots`, `Saved/MovieRenders`, `Saved/VideoCaptures` и внутреннего каталога запуска. Готовность плана меняется на `validated` только после успешного playtest.

## Состояние и Project Map

- состояние: `assets/generated/leetcode/unreal/gameplay/state.json`;
- manifests/results: `assets/generated/leetcode/unreal/gameplay/manifests`;
- run records: `assets/generated/leetcode/unreal/gameplay/runs`;
- планы: `docs/gameplay`.

Project Map получает узлы `GameplayPlan` и `GameplayRun`. План связан с выбранным проектным узлом, task IDs и roadmap IDs, а связь `Tests` ведёт к конкретному запуску. Двойной клик по узлу открывает Markdown-план или первый артефакт запуска.

## Ручная проверка в Unreal Engine 5.8

1. Откройте Unreal workspace и убедитесь, что `unreal_snapshot` видит `.uproject` и `UnrealEditor-Cmd`.
2. Создайте план через кнопку `Unreal Gameplay -> Новый gameplay-план` в панели проекта.
3. Начните с тестовой карты в отдельной ветке. Проверьте manifest до подтверждения.
4. Примените простую операцию: создайте карту, добавьте Actor/Component и сохраните уровень.
5. Запустите project Automation test или `map_smoke` со screenshot.
6. Для видео заранее создайте Level Sequence и MRQ preset, затем вызовите `movie_render` с их object paths.
7. Обновите Project Map и проверьте связь `GameplayPlan -> Tests -> GameplayRun`.

Live-проверка требует установленного Unreal Engine 5.8, совместимого тестового проекта и включённых Editor Scripting/Python/MRQ plugins. Успешная Rust-проверка не заменяет открытие изменённой карты в Editor.

## Первичные источники

- [Automation System User Guide](https://dev.epicgames.com/documentation/en-us/unreal-engine/automation-system-user-guide-in-unreal-engine)
- [Run Automation Tests](https://dev.epicgames.com/documentation/en-us/unreal-engine/run-automation-tests-in-unreal-engine)
- [Functional Testing](https://dev.epicgames.com/documentation/unreal-engine/functional-testing-in-unreal-engine?lang=en-US)
- [Taking Screenshots](https://dev.epicgames.com/documentation/en-us/unreal-engine/taking-screenshots-in-unreal-engine?lang=en-US)
- [Command-Line Arguments Reference](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-command-line-arguments-reference)
- [Command-Line Rendering with Movie Render Queue](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-command-line-rendering-with-move-render-queue-in-unreal-engine)
