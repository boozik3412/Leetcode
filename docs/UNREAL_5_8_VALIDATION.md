# Unreal Engine 5.8 Bridge: локальная проверка

Этот checklist проверяет Stage 37 без изменения проекта и отделяет быструю диагностику окружения от тяжёлых build/cook/package операций.

## 1. Диагностика установки

Для текущего plugin-workspace:

```powershell
.\scripts\validate-unreal-5.8.ps1 -Workspace C:\Users\Admin\Documents\UE
```

Ожидаемый результат:

- найден `VibeUE.uplugin` либо `.uproject` выбранного проекта;
- выбран Unreal Engine `5.8.x`;
- найдены `UnrealEditor.exe`, `UnrealEditor-Cmd.exe`, `Build.bat`, `RunUAT.bat` и UnrealBuildTool;
- найден Visual Studio с C++ toolchain;
- ignored-тест `validates_configured_unreal_5_8_workspace` завершён успешно.

Если движок установлен нестандартно, перед проверкой задайте:

```powershell
$env:UE_ENGINE_ROOT = "D:\Epic\UE_5.8"
```

## 2. Проверка UI

1. Откройте Unreal-проект или plugin-workspace через кнопку `Открыть проект`.
2. Перейдите в `Проект` и раскройте блок быстрых команд.
3. Убедитесь, что `Unreal Bridge` показывает выбранный UE, проект/плагин и состояние toolchain.
4. Наведите курсор на компоненты toolchain: tooltip должен показать путь и диагностическую деталь.
5. Нажмите `Обновить` после изменения `EngineAssociation` или установки workload.

## 3. Безопасные команды

Проверяйте по очереди, начиная с дешёвых операций:

1. `Проверить контент` запускает headless Data Validation.
2. `Файлы IDE` генерирует project files только для `.uproject`.
3. `Собрать Editor` собирает `<ProjectName>Editor Win64 Development`.
4. `Automation` запускает filter `Project` через `UnrealEditor-Cmd` и сохраняет отчёт в `Saved/AutomationReports`.
5. `Открыть Editor` запускает обнаруженный Editor отдельным процессом.
6. `Cook` выполняет unattended cook для Win64.
7. `Package` пишет архив в workspace-local `Packaged`.
8. `Собрать плагин` использует UAT BuildPlugin и пишет результат в `Packaged/Plugin`.

Build, tests, cook и package могут занимать продолжительное время. Все профили проходят обычный approval gate shell-команд, поддерживают отмену и имеют таймаут 30 минут.

## 4. Проверка агентного цикла

Отправьте агенту:

> Проверь Unreal-окружение проекта и выполни Data Validation. Если есть ошибки, перечисли первую первичную причину и предложи следующий шаг.

В журнале должны появиться `unreal_snapshot`, затем `unreal_command`. При ошибках ответ инструмента содержит `issues` с severity, source, file/line (когда их дал компилятор), message и suggested_action.

## 5. Критерий готовности

- fixture-тесты и полный `cargo test` проходят;
- локальный UE 5.8 snapshot проходит;
- хотя бы Data Validation либо BuildPlugin реально запускается после подтверждения;
- отмена длительной команды возвращает управление приложению;
- произвольная shell-строка не может быть передана через `unreal_command`.
