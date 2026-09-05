---
title: "Web builtins"
description: "Builtins in the Web category."
sidebar:
  order: 120
---

## Web builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`error_log()`](./web/error_log.md) | `(string $message, int $message_type = 0, ?string $destination = null, ?string $additional_headers = null): bool` | `bool` | ✓ | — |
| [`ini_get()`](./web/ini_get.md) | `(string $option): mixed` | `mixed` | ✓ | — |
| [`ini_get_all()`](./web/ini_get_all.md) | `(?string $extension = null, bool $details = true): mixed` | `mixed` | ✓ | — |
| [`ini_set()`](./web/ini_set.md) | `(string $option, mixed $value): mixed` | `mixed` | ✓ | — |
| [`session_abort()`](./web/session_abort.md) | `(): bool` | `bool` | ✓ | — |
| [`session_cache_expire()`](./web/session_cache_expire.md) | `(?int $value = null): mixed` | `mixed` | ✓ | — |
| [`session_cache_limiter()`](./web/session_cache_limiter.md) | `(?string $value = null): mixed` | `mixed` | ✓ | — |
| [`session_commit()`](./web/session_commit.md) | `(): bool` | `bool` | ✓ | — |
| [`session_create_id()`](./web/session_create_id.md) | `(string $prefix = ''): mixed` | `mixed` | ✓ | — |
| [`session_decode()`](./web/session_decode.md) | `(string $data): bool` | `bool` | ✓ | — |
| [`session_destroy()`](./web/session_destroy.md) | `(): bool` | `bool` | ✓ | — |
| [`session_encode()`](./web/session_encode.md) | `(): mixed` | `mixed` | ✓ | — |
| [`session_gc()`](./web/session_gc.md) | `(): mixed` | `mixed` | ✓ | — |
| [`session_get_cookie_params()`](./web/session_get_cookie_params.md) | `(): array` | `array` | ✓ | — |
| [`session_id()`](./web/session_id.md) | `(?string $id = null): mixed` | `mixed` | ✓ | — |
| [`session_module_name()`](./web/session_module_name.md) | `(?string $module = null): mixed` | `mixed` | ✓ | — |
| [`session_name()`](./web/session_name.md) | `(?string $name = null): mixed` | `mixed` | ✓ | — |
| [`session_regenerate_id()`](./web/session_regenerate_id.md) | `(bool $delete_old = false): bool` | `bool` | ✓ | — |
| [`session_register_shutdown()`](./web/session_register_shutdown.md) | `(): void` | `void` | ✓ | — |
| [`session_reset()`](./web/session_reset.md) | `(): bool` | `bool` | ✓ | — |
| [`session_save_path()`](./web/session_save_path.md) | `(?string $path = null): mixed` | `mixed` | ✓ | — |
| [`session_set_cookie_params()`](./web/session_set_cookie_params.md) | `(...$args): bool` | `bool` | ✓ | — |
| [`session_set_save_handler()`](./web/session_set_save_handler.md) | `(mixed $handler_or_open = null, mixed $register_or_close = true, mixed $read = null, mixed $write = null, mixed $destroy = null, mixed $gc = null, mixed $create_sid = null, mixed $validate_id = null, mixed $update_timestamp = null): bool` | `bool` | ✓ | — |
| [`session_start()`](./web/session_start.md) | `(mixed $options = []): bool` | `bool` | ✓ | — |
| [`session_status()`](./web/session_status.md) | `(): int` | `int` | ✓ | — |
| [`session_unset()`](./web/session_unset.md) | `(): bool` | `bool` | ✓ | — |
| [`session_write_close()`](./web/session_write_close.md) | `(): bool` | `bool` | ✓ | — |
| [`setcookie()`](./web/setcookie.md) | `(mixed $name, mixed $value = '', mixed $expires = 0, mixed $path = '', mixed $domain = '', mixed $secure = false, mixed $httponly = false): mixed` | `mixed` | ✓ | — |
| [`setrawcookie()`](./web/setrawcookie.md) | `(mixed $name, mixed $value = '', mixed $expires = 0, mixed $path = '', mixed $domain = '', mixed $secure = false, mixed $httponly = false): mixed` | `mixed` | ✓ | — |
| [`trigger_error()`](./web/trigger_error.md) | `(string $message, int $error_level = E_USER_NOTICE): bool` | `bool` | ✓ | — |
