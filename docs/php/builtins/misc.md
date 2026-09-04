---
title: "Misc builtins"
description: "Builtins in the Misc category."
sidebar:
  order: 118
---

## Misc builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`constant()`](./misc/constant.md) | `(string $name): mixed` | `mixed` | ✓ | ✓ |
| [`define()`](./misc/define.md) | `(string $constant_name, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`defined()`](./misc/defined.md) | `(string $constant_name): bool` | `bool` | ✓ | ✓ |
| [`empty()`](./misc/empty.md) | `(mixed $value): bool` | `bool` | ✓ | ✓ |
| [`extension_loaded()`](./misc/extension_loaded.md) | `(string $extension): bool` | `bool` | ✓ | ✓ |
| [`get_loaded_extensions()`](./misc/get_loaded_extensions.md) | `(bool $zend_extensions = false): array` | `array` | ✓ | ✓ |
| [`header()`](./misc/header.md) | `(string $header, bool $replace = true, int $response_code = 0): void` | `void` | ✓ | ✓ |
| [`http_response_code()`](./misc/http_response_code.md) | `(int $response_code = 0): int` | `int` | ✓ | ✓ |
| [`ini_restore()`](./misc/ini_restore.md) | `(string $option): void` | `void` | ✓ | — |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): bool` | `bool` | ✓ | ✓ |
| [`opcache_compile_file()`](./misc/opcache_compile_file.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_get_configuration()`](./misc/opcache_get_configuration.md) | `(): mixed` | `mixed` | ✓ | — |
| [`opcache_get_status()`](./misc/opcache_get_status.md) | `(mixed $include_scripts = true): mixed` | `mixed` | ✓ | — |
| [`opcache_invalidate()`](./misc/opcache_invalidate.md) | `(mixed $filename, mixed $force = false): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached()`](./misc/opcache_is_script_cached.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached_in_file_cache()`](./misc/opcache_is_script_cached_in_file_cache.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_jit_blacklist()`](./misc/opcache_jit_blacklist.md) | `(mixed $closure): void` | `void` | ✓ | — |
| [`opcache_reset()`](./misc/opcache_reset.md) | `(): bool` | `bool` | ✓ | — |
| [`php_sapi_name()`](./misc/php_sapi_name.md) | `(): string` | `string` | ✓ | — |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode = 'a'): string` | `string` | ✓ | ✓ |
| [`phpversion()`](./misc/phpversion.md) | `(string $extension = null): string|false` | `string|false` | ✓ | ✓ |
| [`print_r()`](./misc/print_r.md) | `(mixed $value, bool $return = false): mixed` | `mixed` | ✓ | ✓ |
| [`serialize()`](./misc/serialize.md) | `(mixed $value): string` | `string` | ✓ | — |
| [`unserialize()`](./misc/unserialize.md) | `(string $data, mixed $options = []): mixed` | `mixed` | ✓ | — |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` | ✓ | ✓ |
| [`var_dump()`](./misc/var_dump.md) | `(mixed $value, ...$values): void` | `void` | ✓ | ✓ |
| [`zend_version()`](./misc/zend_version.md) | `(): string` | `string` | ✓ | — |
