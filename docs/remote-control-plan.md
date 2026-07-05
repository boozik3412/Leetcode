# Remote Control Plan

Цель: дать Leetcode безопасное удалённое управление без AnyDesk/VNC: с другого компьютера через тонкий клиент и с iPhone через мобильный интерфейс.

## Этап 25A - Remote Control Foundation

Фокус: личный и приватный доступ к уже запущенному локальному агенту.

Что делаем:

- Встроенный локальный Remote API server внутри desktop-приложения.
- Сервер выключен по умолчанию и слушает `127.0.0.1`.
- Доступ к состоянию через bearer token.
- Endpoint'ы первого прохода:
  - `GET /health` - проверка сервиса.
  - `GET /api/state` - снимок состояния агента и проекта.
  - `GET /api/events` - SSE-поток состояния.
  - `GET /` - лёгкая web/PWA-панель для мобильного браузера.
  - `GET /manifest.webmanifest` - основа для установки на iPhone Home Screen.
- Панель настроек в Leetcode: включить/выключить remote, host, port, token, URL и статус.
- Документация доступа через Tailscale Serve или Cloudflare Tunnel/Access.

Ограничения этапа:

- Удалённый UI пока читает состояние и готовит канал событий.
- Выполнение задач, approvals и просмотр файлов пойдут следующим проходом поверх того же API.
- Прямой публичный доступ без Zero Trust/VPN не рекомендуется.

Критерии готовности:

- Приложение стартует с remote выключенным.
- При включении remote появляется локальный URL.
- `/health`, `/api/state`, `/api/events` отвечают.
- `/api/state` и `/api/events` требуют token.
- Сборка и тесты проходят.

## Этап 25B - Product Remote Access

Фокус: продуктовый сценарий "подключиться по Agent ID" с тонкими клиентами.

Что делаем:

- Leetcode Relay: облачный сервис для исходящих WSS-соединений от host app.
- Agent ID для каждого основного агента.
- Pairing flow:
  - одноразовый код или QR;
  - подтверждение на host app;
  - device token для доверенного клиента;
  - revoke devices.
- Тонкий desktop-клиент на Tauri/WebView.
- iPhone PWA как основной мобильный клиент.
- Позже native iOS app при необходимости APNs, Keychain/passkeys и более стабильного фонового UX.
- Remote actions:
  - Done local-first: отправить задачу агенту через `POST /api/tasks`;
  - Done local-first: approve/deny план перед запуском через `POST /api/run-gate`;
  - Done local-first: approve/deny действие инструмента через `POST /api/approval`;
  - Done local-first: смотреть tool log через `GET /api/tool-log`;
  - Done local-first: смотреть run history через `GET /api/history`;
  - Done local-first: смотреть подробный run timeline через `GET /api/history/run?id=...`;
  - Done local-first: читать UTF-8 файлы текущей workspace в безопасном read-only режиме через `GET /api/files/content?path=...`;
  - Done local-first: смотреть safe-команды палитры через `GET /api/commands`;
  - Done local-first: запускать safe-команды палитры через `POST /api/commands`;
  - later product: запускать макросы/проектные команды по отдельным ролям, risk-классам и подтверждениям.
- Security hardening:
  - short-lived access tokens;
  - passkeys/WebAuthn;
  - TLS/WSS;
  - Done local-first: Origin validation с allowlist в настройках;
  - Done local-first: глобальный rate limit для Remote API;
  - Done local-first: audit log remote actions;
  - Done local-first: roles: view/chat/approve/files;
  - later product: per-device/per-IP limits, revoke devices, run/desktop roles, signed short-lived sessions.

Критерии готовности:

- Пользователь может подключиться к агенту по Agent ID.
- Новый клиент должен быть подтверждён владельцем.
- iPhone PWA показывает live-состояние и позволяет подтвердить действие.
- Desktop thin client работает на другом компьютере.
- Все опасные действия проходят через существующую permission system.

## Рекомендованный порядок

1. Закрыть 25A как локальный/private-first фундамент.
2. Добавить remote actions поверх уже работающего API.
3. Реализовать 25B Relay/Agent ID.
4. После первых реальных поездок и использования с iPhone решить, нужен ли native iOS app.
