# Vertical Slice Orchestrator

Stage 44 превращает набор игровых инструментов Leetcode в единый воспроизводимый цикл создания вертикального среза Unreal-проекта. Оркестратор не заменяет Game Production Director: production-план отвечает за весь проект, а Vertical Slice run ведёт один репрезентативный игровой срез от проверки окружения до production gate.

## Состояние

Состояние хранится внутри workspace:

```text
assets/generated/leetcode/vertical-slice/state.json
```

Run связан с production plan и содержит фазы, зависимости, evidence, workspace-relative artifact, историю переходов, текущий статус и время обновления. После перезапуска приложения агент продолжает тот же незавершённый run.

## Граф фаз

```text
Preflight
  |-- Gameplay Foundation --|
  |-- Visual Assets --------|-> Level Integration
                                  -> Experience
                                  -> Playtest
                                  -> Production Gate
```

После Preflight фазы Gameplay Foundation и Visual Assets становятся ready одновременно. Их можно передать двум ограниченным субагентам. Главный агент проверяет результаты, сохраняет evidence/artifacts и единолично обновляет orchestration state. Level Integration не разблокируется, пока не завершены обе ветки.

## Инструменты агента

- `vertical_slice_snapshot` — прочитать активный run, ready-фазы и рекомендуемые инструменты.
- `start_vertical_slice_run` — создать run для production plan со scope `vertical_slice` или `full_game`.
- `advance_vertical_slice_phase` — начать, заблокировать или завершить фазу с доказательствами.
- `evaluate_vertical_slice_readiness` — пересчитать gates, blockers, прогресс и следующий допустимый шаг.

Изменяющие состояние операции проходят governance и approval. Статусы `planned` и `ready` вычисляются оркестратором и не назначаются агентом вручную.

## Gates

- Preflight: найден `.uproject`, выбран Unreal Engine и зарегистрирован хотя бы один MCP server.
- Gameplay Foundation: существует применённый или проверенный gameplay plan.
- Visual Assets: существует import-ready 3D job либо подтверждённый артефакт.
- Level Integration и Experience: приложен существующий файл внутри workspace.
- Playtest: есть успешный gameplay run.
- Production Gate: пройден Vertical Slice gate Game Production Director.

Фаза не может завершиться без краткого evidence или существующего workspace artifact. Зависимости нельзя пропустить вручную.

## Интерфейс и Project Map

В `Проект -> Game Production` показаны прогресс run, следующая фаза, параллельно готовые ветки и действия `Продолжить`, `Проверить readiness`, `Распараллелить` и `Агенту`. Project Map содержит run, фазы, зависимости и ссылки на созданные артефакты.

## Проверка

```powershell
.\.cargo\bin\cargo.exe test --bin leetcode vertical_slice
powershell -ExecutionPolicy Bypass -File scripts/production-preflight.ps1 -Mode Quick
```

Live-проверки Unreal, MCP и платных asset providers выполняются только явно настроенным production workspace.
