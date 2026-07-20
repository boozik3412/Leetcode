# 3D Asset Pipeline

Stage 40 добавляет воспроизводимый путь `идея/изображение -> внешний 3D provider -> локальный пакет -> проверка -> Unreal Engine 5.8`.

## Провайдеры

| Провайдер | ID | Ключ | Модель по умолчанию | Вход |
| --- | --- | --- | --- | --- |
| Meshy | `meshy-3d` | `MESHY_API_KEY` | `latest` | text-to-3D, image-to-3D |
| Tripo | `tripo-3d` | `TRIPO_API_KEY` | `P1-20260311` | text-to-3D, image-to-3D |

Ключ можно сохранить в `Asset Studio -> 3D`; он использует общее защищённое хранилище настроек провайдеров и не записывается в проект. HTTP-запросы наследуют proxy-настройки Leetcode.

Провайдеры работают асинхронно. Отправка создаёт локальный job и возвращает provider task ID, а `Обновить статус` опрашивает API. Meshy text-to-3D автоматически проходит preview/refine для PBR. Готовый результат загружается в проект только после успешного статуса провайдера.

Официальные контракты API:

- [Meshy Text to 3D](https://docs.meshy.ai/en/api/text-to-3d)
- [Meshy Image to 3D](https://docs.meshy.ai/en/api/image-to-3d)
- [Tripo generation tasks](https://platform.tripo3d.ai/docs/generation)
- [Tripo file upload](https://platform.tripo3d.ai/docs/upload)

## Asset Studio

1. Откройте вкладку `Ассеты` и выберите тип `3D`.
2. Выберите Meshy или Tripo, сохраните ключ и модель.
3. Введите описание. Для image-to-3D выберите PNG/JPEG/WebP; внешний файл будет скопирован в безопасную папку вложений проекта.
4. Выберите `GLB`, `glTF`, `FBX` или `USD`, целевой поликаунт, PBR и при необходимости A/T-pose.
5. Подтвердите право использовать входные данные и результат провайдера.
6. Отправьте job и обновляйте его до состояния `готово`.
7. Нажмите `Проверить`. Кнопка `В Unreal` доступна только для `import-ready` результата и готовит точную задачу агенту.

Jobs сохраняются в `assets/generated/leetcode/assets3d/jobs.json`. Результаты находятся в `assets/generated/3d`. Рядом с каждым скачанным файлом создаётся `<asset>.<ext>.asset.json` с provider/model/task ID, ссылкой на условия, временем и подтверждением лицензии.

## Инструменты агента

- `asset_3d_snapshot` — провайдеры и все сохранённые jobs.
- `submit_3d_asset` — text/image-to-3D; платный вызов требует подтверждения по policy.
- `refresh_3d_asset` — получить прогресс, скачать готовый файл и provenance, запустить проверку.
- `validate_3d_asset` — повторная локальная проверка без API.
- `import_3d_asset_unreal` — безопасный headless импорт через UnrealEditor-Cmd и Python.

Агенту запрещено считать ассет готовым только по ответу провайдера. Последовательность: `submit -> refresh -> validate -> import`.

## Локальная проверка

Валидатор не отправляет проект наружу. Он проверяет:

- GLB/glTF header и JSON, meshes/primitives, vertices и triangles;
- UV0, normals, tangents, PBR materials и textures;
- bounds и заметку о масштабе;
- FBX/USD структуру и наличие mesh geometry;
- соседний PBR texture set;
- LOD, рекомендацию Nanite и collision;
- skins/rig и animations как независимые стадии;
- provenance и явное подтверждение лицензии.

`import_ready` требует валидную геометрию, provenance, лицензию и отсутствие ошибок. Rig/skeleton и animation не синтезируются молча: отсутствие данных отражается в отчёте.

## Unreal Engine 5.8

`import_3d_asset_unreal`:

1. повторно проверяет исходник и лицензию;
2. обнаруживает `.uproject` и UnrealEditor-Cmd;
3. записывает manifest/result в `assets/generated/leetcode/unreal/imports`;
4. запускает `scripts/unreal/import_3d_asset.py`;
5. импортирует или переимпортирует ассет в `/Game/Generated/Leetcode`;
6. сохраняет импортированные packages и возвращает структурированный результат.

GLB/glTF/USD проходят через настроенный в проекте Interchange pipeline. Для FBX скрипт задаёт static mesh, skeletal mesh или animation options, LOD, skeleton, Nanite и collision. Нужны включённые Python Editor Script Plugin и соответствующие Interchange/import plugins проекта.

- [Unreal Engine 5.8 Interchange import](https://dev.epicgames.com/documentation/unreal-engine/importing-assets-using-interchange-in-unreal-engine?lang=en-US)
- [Unreal Editor Python scripting](https://dev.epicgames.com/documentation/unreal-engine/scripting-the-unreal-editor-using-python)

## Проверка

Локально:

```powershell
.\.cargo\bin\cargo.exe fmt --all -- --check
.\.cargo\bin\cargo.exe check
.\.cargo\bin\cargo.exe test asset_3d
python -m py_compile scripts/unreal/import_3d_asset.py
```

Live-проверка требует оплаченного ключа выбранного провайдера. Она должна запускаться вручную на небольшом ассете. Импорт в Unreal проверяется на отдельном тестовом `.uproject`; наличие успешного HTTP task не заменяет проверку результата в Editor.
