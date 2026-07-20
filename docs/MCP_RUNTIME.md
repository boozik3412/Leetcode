# MCP Runtime и Unreal Engine 5.8

Leetcode подключает MCP-серверы через официальный Rust SDK `rmcp`. Поддерживаются два стандартных транспорта:

- `stdio` для локального дочернего процесса;
- Streamable HTTP с MCP session lifecycle, автоматической повторной инициализацией истёкшей HTTP-сессии и одним reconnect после ошибки вызова.

## Реестр серверов

Для выбранного проекта используется файл:

```text
assets/generated/leetcode/mcp/servers.json
```

Если рабочая папка похожа на Unreal-проект или standalone Unreal-плагин, Leetcode создаёт профиль `unreal-mcp`:

```json
{
  "version": 1,
  "servers": [
    {
      "id": "unreal-mcp",
      "label": "Unreal Engine 5.8 MCP",
      "enabled": true,
      "transport": "streamable_http",
      "url": "http://127.0.0.1:8000/mcp",
      "command": null,
      "args": [],
      "cwd": null,
      "env_vars": [],
      "bearer_token_env": null,
      "allowed_tools": ["call_tool", "describe_toolset", "list_toolsets"],
      "require_approval": true,
      "allow_remote": false,
      "timeout_secs": 120
    }
  ]
}
```

Секреты в реестре не сохраняются. `env_vars` содержит только имена переменных окружения для `stdio`, а `bearer_token_env` указывает имя переменной с bearer token для удалённого HTTP-сервера.

Пример локального `stdio`-сервера:

```json
{
  "id": "local-tools",
  "label": "Local project tools",
  "enabled": true,
  "transport": "stdio",
  "command": "npx",
  "args": ["-y", "@example/project-mcp"],
  "cwd": ".",
  "env_vars": ["PROJECT_MCP_API_KEY"],
  "bearer_token_env": null,
  "allowed_tools": ["inspect_project"],
  "require_approval": true,
  "allow_remote": false,
  "timeout_secs": 120
}
```

Удалённый Streamable HTTP разрешается только при `allow_remote: true`, URL с `https://` и заданном `bearer_token_env`.

## Unreal MCP

В Unreal Engine 5.8:

1. Откройте `Edit -> Plugins`.
2. Включите экспериментальный плагин `Unreal MCP` (`ModelContextProtocol`).
3. Убедитесь, что зависимый `Toolset Registry` включён.
4. Перезапустите Editor.
5. В Output Log выполните `MCP.StartServer 8000`, если сервер не стартовал автоматически.
6. Для полного набора возможностей добавьте нужные toolsets, включая `AllToolsets`, и выполните `MCP.RefreshTools`.

Нативный сервер Unreal слушает только loopback endpoint `http://127.0.0.1:8000/mcp`. Он не предназначен для публикации в интернет: у него нет собственной аутентификации. Leetcode намеренно отключает proxy для loopback MCP.

Unreal выполняет MCP-вызовы в game thread, поэтому Leetcode сериализует вызовы. Агент использует последовательность:

1. `mcp_snapshot`;
2. `mcp_discover` с `server: "unreal-mcp"`;
3. `mcp_call` / `list_toolsets`;
4. `mcp_call` / `describe_toolset`;
5. `mcp_call` / `call_tool` после подтверждения пользователя.

## Модель доверия

- MCP config проходит локальную валидацию, дубликаты ID запрещены.
- Пустой `allowed_tools` запрещает подключение; неизвестный tool call отклоняется до сети.
- `stdio` discovery и любой удалённый discovery требуют подтверждения.
- Вызов инструмента требует подтверждения, если это задано сервером или режимом доступа приложения.
- Аргументы ограничены 100 KB, результат 120 000 символами, timeout 5-900 секунд.
- MCP tool metadata и output всегда считаются недоверенными.
- Результат оборачивается в `<untrusted_mcp_output>` и не может менять системные инструкции, план или разрешения агента.
- В технический журнал записываются server/tool, решение approval, размер аргументов, результат и ошибка; значения секретов не пишутся.

## Локальная проверка

1. Выберите Unreal 5.8 проект в Leetcode.
2. Откройте `Код -> Проект` и нажмите `Обновить`.
3. В блоке `MCP Bridge` должен появиться `Unreal Engine 5.8 MCP`.
4. Запустите Unreal Editor и MCP server.
5. Нажмите `Проверить`, отправьте подготовленный запрос агенту и подтвердите discovery.
6. Проверьте, что сервер показывает `подключён`, protocol version и объявленные tools.
7. Попросите агента показать toolsets. Подтвердите `mcp_call` и убедитесь, что результат помечен как недоверенный.
8. Остановите MCP сервер и повторите вызов: UI и журнал должны показать ошибку, а следующий вызов после запуска сервера должен переподключиться.

Полезные команды проекта:

```powershell
.\run-leetcode.cmd check
.\run-leetcode.cmd test
.\.cargo\bin\cargo.exe test mcp::tests::validates_live_unreal_mcp_when_opted_in -- --ignored --nocapture
```

Последняя команда обращается к `http://127.0.0.1:8000/mcp`. Другой endpoint можно передать через `LEETCODE_UE_MCP_URL`.

Актуальные первичные источники: документация Unreal Engine 5.8 по Unreal MCP, спецификация MCP 2025-11-25 и официальный `modelcontextprotocol/rust-sdk`.
