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
| [`error_reporting()`](./misc/error_reporting.md) | `(int $error_level = null): int` | `int` | ✓ | ✓ |
| [`extension_loaded()`](./misc/extension_loaded.md) | `(string $extension): bool` | `bool` | ✓ | ✓ |
| [`gc_collect_cycles()`](./misc/gc_collect_cycles.md) | `(): int` | `int` | ✓ | — |
| [`gc_enable()`](./misc/gc_enable.md) | `(): void` | `void` | ✓ | — |
| [`get_extension_funcs()`](./misc/get_extension_funcs.md) | `(string $extension): mixed` | `mixed` | ✓ | ✓ |
| [`get_loaded_extensions()`](./misc/get_loaded_extensions.md) | `(bool $zend_extensions = false): array` | `array` | ✓ | ✓ |
| [`header()`](./misc/header.md) | `(string $header, bool $replace = true, int $response_code = 0): void` | `void` | ✓ | ✓ |
| [`http_response_code()`](./misc/http_response_code.md) | `(int $response_code = 0): int` | `int` | ✓ | ✓ |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): bool` | `bool` | ✓ | ✓ |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode = 'a'): string` | `string` | ✓ | ✓ |
| [`phpversion()`](./misc/phpversion.md) | `(string $extension = null): string|false` | `string|false` | ✓ | ✓ |
| [`print_r()`](./misc/print_r.md) | `(mixed $value, bool $return = false): mixed` | `mixed` | ✓ | ✓ |
| [`serialize()`](./misc/serialize.md) | `(mixed $value): string` | `string` | ✓ | — |
| [`setlocale()`](./misc/setlocale.md) | `(int $category, mixed $locales, ...$rest): mixed` | `mixed` | ✓ | ✓ |
| [`unserialize()`](./misc/unserialize.md) | `(string $data, mixed $options = []): mixed` | `mixed` | ✓ | — |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` | ✓ | ✓ |
| [`var_dump()`](./misc/var_dump.md) | `(mixed $value, ...$values): void` | `void` | ✓ | ✓ |
