# Game Production Director

Stage 43 добавляет в Leetcode единый управляемый цикл производства игры:

`идея -> scope -> milestone -> workstreams -> зависимости -> артефакты -> gate`

Он не заменяет gameplay, 3D, MCP и Unreal-инструменты. Production Director связывает их в один план и помогает агенту выбирать следующий проверяемый шаг.

## Масштабы проекта

- `prototype` — игровое видение, технический фундамент, core loop, blockout и проверка прототипа;
- `vertical_slice` — всё из прототипа плюс визуальный target, 3D, анимация, уровень, UI, audio, интеграция и playtest;
- `full_game` — полный маршрут через Prototype, Vertical Slice, Alpha, Beta и Release.

Каждый план фиксирует Unreal Engine 5.8, жанр, целевую платформу, выбранный узел Project Map, связанные task и roadmap entries.

## Инструменты агента

- `game_production_snapshot` — читает все планы и состояние milestone gates;
- `create_game_production_plan` — создаёт persistent plan из brief и выбранного scope;
- `update_production_item` — меняет состояние задачи и прикладывает validation или существующий artifact;
- `evaluate_production_gate` — проверяет готовность milestone.

Изменение плана проходит через write approval и governance. Задачу нельзя перевести в `done`, пока у неё нет текста реальной проверки или существующего workspace-relative артефакта.

## Milestone gates

Gate требует завершения всех задач текущего milestone. Начиная с Vertical Slice также нужен успешный gameplay playtest. Для Release дополнительно требуется зелёный Stage 42 production validation report.

## Интерфейс

В режиме `Проект` блок `Game Production` показывает:

- активный план, scope, milestone и общий прогресс;
- текущие незавершённые задачи и их workstream;
- зависимости и статус задачи;
- действия `В работу`, `Агенту`, `Продолжить с агентом` и `Проверить gate`.

В Project Map доступны узлы `production-план` и `production-задача`. Выбор такого узла попадает в следующий prompt агента, поэтому задачу можно ставить по конкретному элементу производственного плана.

## Хранение

Состояние проекта сохраняется в:

`assets/generated/leetcode/game-production/state.json`

Файл можно версионировать вместе с проектом или исключить из Git в зависимости от политики команды. Он не содержит API-ключей.

## Рекомендуемый цикл

1. Создать план нужного масштаба в Project Command Center.
2. Попросить агента продолжить план или выбрать конкретную production-задачу на карте.
3. Выполнить работу через coding, asset, MCP и Unreal-пайплайны.
4. Сохранить validation и артефакт через `update_production_item`.
5. Запустить `evaluate_production_gate`.
6. После зелёного gate перейти к следующему milestone.
