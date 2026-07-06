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
  - Done local-first: одноразовый код на host app;
  - Done local-first: device token для доверенного клиента;
  - Done local-first: device registry, роли, last seen и revoke;
  - Done relay-first: QR, pending approval dialog и relay-подтверждение устройства.
- Done local-first: тонкий desktop-клиент как отдельный Rust/egui binary `leetcode-client`.
- Done local-first: Windows installer/portable package для тонкого клиента.
- Done relay-first: тонкий desktop-клиент с Agent ID/pending pairing UX поверх relay.
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
  - Done foundation: запускать макросы/проектные команды с risk-классами, preview, confirmation и approve-gate для high-risk действий.
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

## Direct-подключение тонкого клиента

Direct-режим остаётся полезным для локальной сети, VPN и отладки:

1. На основном компьютере открыть Leetcode.
2. Включить Remote API в настройках удалённого доступа.
3. Нажать `Новый код подключения` в блоке `Подключение устройств`.
4. Установить `Leetcode Client` на другом Windows-компьютере.
5. Нажать `Копировать паспорт` в основном Leetcode.
6. В Leetcode Client нажать `Вставить паспорт`, затем `Подключить по коду`.
7. Клиент получит отдельный device token и сохранит его локально.

Паспорт подключения содержит только Remote URL, Agent ID и короткоживущий pairing code. После подключения host app показывает доверенное устройство, last seen, revoke и роли устройства: обзор, задачи, подтверждения, файлы. Через клиент можно отправить задачу, выполнить safe remote command и подтвердить или отклонить ожидающий план/действие инструмента.

## Relay MVP

Stage 28 добавляет первый рабочий relay-слой без ручного Remote URL:

1. Запустить `leetcode-relay.exe --bind 0.0.0.0:17990` на доступной машине.
2. В основном Leetcode открыть `Контроль -> Удалённый доступ`.
3. Включить `Relay`, указать Relay URL и сохранить.
4. Создать `Новый код подключения` и нажать `Копировать паспорт`.
5. В Leetcode Client включить `Relay по Agent ID`, вставить паспорт и нажать `Подключить по коду`.
6. В основном Leetcode подтвердить или отклонить новый pending-запрос устройства.
7. В Leetcode Client нажать `Проверить подтверждение`; после одобрения клиент сохранит свой device token.

Архитектура MVP: host app отправляет на relay `POST /api/hosts/poll` с Agent ID, host token, snapshot и активным pairing code. Клиент вызывает `/api/clients/pair`, получает `pending request_id`, затем опрашивает `/api/clients/pair/status` до решения host app. Host app получает pending-запрос как relay action и отправляет решение в `/api/hosts/pairing/decision`. После подтверждения клиент использует `/api/clients/state`, `/api/clients/tasks`, `/api/clients/commands`, `/api/clients/run-gate`, `/api/clients/approval`. Relay хранит только оперативное состояние в памяти и очередь действий; после перезапуска relay устройства нужно переподключить.

Stage 28B добавляет слой диагностики поверх Relay MVP:

- Relay считает host online только если последний poll был не старше 15 секунд.
- `GET /health` показывает `host_count`, `online_hosts`, `queued_actions` и TTL host-сессии.
- `/api/clients/state` возвращает `host_online`, `host_updated_at`, `host_age_secs` и `queued_actions`; если host устарел, клиент получает понятный offline-ответ вместо старого snapshot.
- Leetcode Client показывает, работает ли он через direct или relay, online/offline статус host, возраст snapshot и очередь relay-действий.
- Основное приложение показывает последнюю успешную relay-синхронизацию и число полученных действий в панели удалённого доступа.

Локальная smoke-проверка Relay MVP:

1. Запустить relay: `leetcode-relay.exe --bind 127.0.0.1:19090`.
2. Проверить `GET http://127.0.0.1:19090/health`: до host poll должно быть `host_count: 0`.
3. Отправить host poll на `/api/hosts/poll` с тестовым Agent ID, host token, pairing code и snapshot.
4. Выполнить `/api/clients/pair`: ответ должен содержать `status: pending` и `request_id`.
5. Выполнить `/api/hosts/pairing/decision` с host token и `approved: true`.
6. Выполнить `/api/clients/pair/status`: ответ должен содержать `status: approved` и device token.
7. Выполнить `/api/clients/state` с device token: ответ должен содержать `host_online: true`, `host_age_secs` и `queued_actions`.
8. Отправить `/api/clients/tasks`, затем снова `/api/hosts/poll`: host должен получить действие `submit_task`.
9. Подождать больше 15 секунд без host poll и повторить `/api/clients/state`: relay должен вернуть offline-состояние.

## iPhone/PWA через Relay

Stage 29 добавляет мобильную PWA прямо в `leetcode-relay`:

1. Запустить `leetcode-relay.exe --bind 0.0.0.0:17990` на доступной машине.
2. В основном Leetcode включить `Relay` и дождаться успешной синхронизации.
3. Создать новый pairing code в `Контроль -> Удалённый доступ -> Подключение устройств`.
4. Нажать `Ссылка iPhone` или открыть QR-код камерой iPhone.
5. Открыть ссылку на iPhone. Она заполнит Agent ID и временный pairing code.
6. Нажать `Подключить по коду`; PWA создаст pending-запрос устройства.
7. В основном Leetcode подтвердить устройство и роли доступа.
8. На iPhone нажать `Проверить подтверждение`, если PWA не обновилась автоматически; после одобрения PWA сохранит device token в localStorage iPhone.

Мобильная PWA показывает online/offline статус host, возраст snapshot, текущий проект, модель, последние запуски, tail логов и безопасные команды. Через неё можно отправить задачу агенту и подтвердить или отклонить ожидающий план/действие. По умолчанию iPhone получает роли `view`, `chat`, `approve` без файлового доступа; роли можно отозвать или изменить в host app.

Прямой URL PWA: `http://relay-host:17990/`. Если ссылка открыта без query-параметров, Agent ID, pairing code и device token можно ввести вручную.

Следующий продуктовый слой должен заменить HTTP long-poll на WSS/TLS, добавить публичный relay deployment, срок действия/rotation device tokens и более подробную диагностику причин offline.

## Рекомендованный порядок

1. Закрыть 25A как локальный/private-first фундамент.
2. Добавить remote actions поверх уже работающего API.
3. Реализовать 25B Relay/Agent ID.
4. После первых реальных поездок и использования с iPhone решить, нужен ли native iOS app.
