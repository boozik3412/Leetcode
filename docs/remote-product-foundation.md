# Remote Product Foundation

Этот документ фиксирует foundation-проход по публичному удалённому управлению Leetcode.

## Relay

- `leetcode-relay.exe` принимает `--public-url`, `--tls-mode`, `--host-session-ttl-secs`, `--client-session-ttl-secs`, `--client-poll-ms`.
- `/health` отдаёт публичный URL, transport, `supports_wss`, TTL host/client session и рекомендованный polling.
- TLS/WSS реализуется через edge/reverse proxy слой: Cloudflare Tunnel, Caddy, Nginx, Tailscale Funnel. Сам relay остаётся маленьким HTTP-процессом.
- Leetcode Client и iPhone PWA используют adaptive polling/backoff: при ошибках сеть не заспамливается запросами.
- `scripts/run-relay-public.ps1` запускает публичный relay profile и входит в thin-client пакет.

Пример запуска:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-relay-public.ps1 `
  -Bind 0.0.0.0:17990 `
  -PublicUrl https://relay.example.com `
  -TlsMode edge
```

## Updater

`latest.json` поддерживает product-grade metadata:

- `rollout_percent` и `rollout_seed` для staged rollout.
- `signature_algorithm` и `signature` для будущей строгой подписи update channel.
- `rollback_version`, `rollback_package`, `rollback_sha256` для будущего UI rollback.
- `minimum_supported_version` для ограничения устаревших клиентов.

Текущая установка проверяет SHA256 пакета и не ставит обновление, если её deterministic Agent ID bucket ещё не входит в staged rollout.

## Следующий Слой

Следующий продуктовый слой: hosted relay с аккаунтами, persistent storage, server-side audit, rotation device tokens, push/websocket events вместо long-poll и UI выбора relay channel.
