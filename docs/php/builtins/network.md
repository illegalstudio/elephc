---
title: "Network builtins"
description: "Builtins in the Network category."
sidebar:
  order: 112
---

## Network builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`curl_close()`](./network/curl_close.md) | `(CurlHandle $handle): void` | `void` | ✓ | ✓ |
| [`curl_copy_handle()`](./network/curl_copy_handle.md) | `(CurlHandle $handle): CurlHandle` | `CurlHandle` | ✓ | ✓ |
| [`curl_errno()`](./network/curl_errno.md) | `(CurlHandle $handle): int` | `int` | ✓ | ✓ |
| [`curl_error()`](./network/curl_error.md) | `(CurlHandle $handle): string` | `string` | ✓ | ✓ |
| [`curl_escape()`](./network/curl_escape.md) | `(CurlHandle $handle, string $string): string` | `string` | ✓ | ✓ |
| [`curl_exec()`](./network/curl_exec.md) | `(CurlHandle $handle): string|bool` | `string|bool` | ✓ | ✓ |
| [`curl_getinfo()`](./network/curl_getinfo.md) | `(CurlHandle $handle, int $option = null): mixed` | `mixed` | ✓ | ✓ |
| [`curl_init()`](./network/curl_init.md) | `(string $url = null): CurlHandle` | `CurlHandle` | ✓ | ✓ |
| [`curl_multi_add_handle()`](./network/curl_multi_add_handle.md) | `(CurlMultiHandle $multi_handle, mixed $handle): int` | `int` | ✓ | ✓ |
| [`curl_multi_close()`](./network/curl_multi_close.md) | `(CurlMultiHandle $multi_handle): void` | `void` | ✓ | ✓ |
| [`curl_multi_errno()`](./network/curl_multi_errno.md) | `(CurlMultiHandle $multi_handle): int` | `int` | ✓ | ✓ |
| [`curl_multi_exec()`](./network/curl_multi_exec.md) | `(CurlMultiHandle $multi_handle, int $still_running): int` | `int` | ✓ | ✓ |
| [`curl_multi_get_handles()`](./network/curl_multi_get_handles.md) | `(CurlMultiHandle $multi_handle): array` | `array` | ✓ | ✓ |
| [`curl_multi_getcontent()`](./network/curl_multi_getcontent.md) | `(mixed $handle): ?string` | `?string` | ✓ | ✓ |
| [`curl_multi_info_read()`](./network/curl_multi_info_read.md) | `(CurlMultiHandle $multi_handle, int $queued_messages = null): mixed` | `mixed` | ✓ | ✓ |
| [`curl_multi_init()`](./network/curl_multi_init.md) | `(): CurlMultiHandle` | `CurlMultiHandle` | ✓ | ✓ |
| [`curl_multi_remove_handle()`](./network/curl_multi_remove_handle.md) | `(CurlMultiHandle $multi_handle, mixed $handle): int` | `int` | ✓ | ✓ |
| [`curl_multi_select()`](./network/curl_multi_select.md) | `(CurlMultiHandle $multi_handle, float $timeout = 1.0): int` | `int` | ✓ | ✓ |
| [`curl_multi_setopt()`](./network/curl_multi_setopt.md) | `(CurlMultiHandle $multi_handle, int $option, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`curl_multi_strerror()`](./network/curl_multi_strerror.md) | `(int $error_code): string` | `string` | ✓ | ✓ |
| [`curl_pause()`](./network/curl_pause.md) | `(CurlHandle $handle, int $flags): int` | `int` | ✓ | ✓ |
| [`curl_reset()`](./network/curl_reset.md) | `(CurlHandle $handle): void` | `void` | ✓ | ✓ |
| [`curl_setopt()`](./network/curl_setopt.md) | `(CurlHandle $handle, int $option, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`curl_setopt_array()`](./network/curl_setopt_array.md) | `(CurlHandle $handle, array $options): bool` | `bool` | ✓ | ✓ |
| [`curl_share_close()`](./network/curl_share_close.md) | `(CurlShareHandle $share_handle): void` | `void` | ✓ | ✓ |
| [`curl_share_errno()`](./network/curl_share_errno.md) | `(CurlShareHandle $share_handle): int` | `int` | ✓ | ✓ |
| [`curl_share_init()`](./network/curl_share_init.md) | `(): CurlShareHandle` | `CurlShareHandle` | ✓ | ✓ |
| [`curl_share_init_persistent()`](./network/curl_share_init_persistent.md) | `(array $share_options): CurlSharePersistentHandle` | `CurlSharePersistentHandle` | ✓ | ✓ |
| [`curl_share_setopt()`](./network/curl_share_setopt.md) | `(CurlShareHandle $share_handle, int $option, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`curl_share_strerror()`](./network/curl_share_strerror.md) | `(int $error_code): string` | `string` | ✓ | ✓ |
| [`curl_strerror()`](./network/curl_strerror.md) | `(int $error_code): string` | `string` | ✓ | ✓ |
| [`curl_unescape()`](./network/curl_unescape.md) | `(CurlHandle $handle, string $string): string` | `string` | ✓ | ✓ |
| [`curl_upkeep()`](./network/curl_upkeep.md) | `(CurlHandle $handle): bool` | `bool` | ✓ | ✓ |
| [`curl_version()`](./network/curl_version.md) | `(): mixed` | `mixed` | ✓ | ✓ |
