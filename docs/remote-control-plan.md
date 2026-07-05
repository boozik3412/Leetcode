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
  - отправить задачу агенту;
  - approve/deny действия;
  - смотреть tool log;
  - смотреть run timeline/history;
  - читать файлы в безопасном read-only режиме;
  - запускать макросы/палитру команд по разрешениям.
- Security hardening:
  - short-lived access tokens;
  - passkeys/WebAuthn;
  - TLS/WSS;
  - Origin validation;
  - rate limiting;
  - audit log remote actions;
  - roles: view/chat/approve/run/files/desktop.

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
