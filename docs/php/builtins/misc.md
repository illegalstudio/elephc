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
| [`debug_backtrace()`](./misc/debug_backtrace.md) | `(int $options = 1, int $limit = 0): array` | `array` | ✓ | ✓ |
| [`debug_print_backtrace()`](./misc/debug_print_backtrace.md) | `(int $options = 0, int $limit = 0): void` | `void` | ✓ | ✓ |
| [`define()`](./misc/define.md) | `(string $constant_name, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`defined()`](./misc/defined.md) | `(string $constant_name): bool` | `bool` | ✓ | ✓ |
| [`empty()`](./misc/empty.md) | `(mixed $value): bool` | `bool` | ✓ | ✓ |
| [`error_reporting()`](./misc/error_reporting.md) | `(mixed $error_level = null): int` | `int` | ✓ | ✓ |
| [`extension_loaded()`](./misc/extension_loaded.md) | `(string $extension): bool` | `bool` | ✓ | ✓ |
| [`func_get_arg()`](./misc/func_get_arg.md) | `(int $position): mixed` | `mixed` | ✓ | ✓ |
| [`func_get_args()`](./misc/func_get_args.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`func_num_args()`](./misc/func_num_args.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_collect_cycles()`](./misc/gc_collect_cycles.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_disable()`](./misc/gc_disable.md) | `(): void` | `void` | ✓ | ✓ |
| [`gc_enable()`](./misc/gc_enable.md) | `(): void` | `void` | ✓ | ✓ |
| [`gc_enabled()`](./misc/gc_enabled.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`gc_mem_caches()`](./misc/gc_mem_caches.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_status()`](./misc/gc_status.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`get_defined_constants()`](./misc/get_defined_constants.md) | `(bool $categorize = false): array` | `array` | ✓ | ✓ |
| [`get_defined_functions()`](./misc/get_defined_functions.md) | `(bool $exclude_disabled = true): array` | `array` | ✓ | ✓ |
| [`get_defined_vars()`](./misc/get_defined_vars.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_extension_funcs()`](./misc/get_extension_funcs.md) | `(string $extension): mixed` | `mixed` | ✓ | ✓ |
| [`get_included_files()`](./misc/get_included_files.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_loaded_extensions()`](./misc/get_loaded_extensions.md) | `(bool $zend_extensions = false): array` | `array` | ✓ | ✓ |
| [`get_mangled_object_vars()`](./misc/get_mangled_object_vars.md) | `(mixed $object): array` | `array` | ✓ | ✓ |
| [`get_required_files()`](./misc/get_required_files.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_resources()`](./misc/get_resources.md) | `(mixed $type = null): array` | `array` | ✓ | ✓ |
| [`header()`](./misc/header.md) | `(string $header, bool $replace = true, int $response_code = 0): void` | `void` | ✓ | ✓ |
| [`http_response_code()`](./misc/http_response_code.md) | `(int $response_code = 0): int` | `int` | ✓ | ✓ |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): bool` | `bool` | ✓ | ✓ |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode = 'a'): string` | `string` | ✓ | ✓ |
| [`phpversion()`](./misc/phpversion.md) | `(string $extension = null): string|false` | `string|false` | ✓ | ✓ |
| [`print_r()`](./misc/print_r.md) | `(mixed $value, bool $return = false): mixed` | `mixed` | ✓ | ✓ |
| [`restore_error_handler()`](./misc/restore_error_handler.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`restore_exception_handler()`](./misc/restore_exception_handler.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`serialize()`](./misc/serialize.md) | `(mixed $value): string` | `string` | ✓ | — |
| [`set_error_handler()`](./misc/set_error_handler.md) | `(mixed $callback, int $error_levels = E_ALL): mixed` | `mixed` | ✓ | ✓ |
| [`set_exception_handler()`](./misc/set_exception_handler.md) | `(mixed $callback): mixed` | `mixed` | ✓ | ✓ |
| [`trigger_error()`](./misc/trigger_error.md) | `(string $message, int $error_level = 1024): bool` | `bool` | ✓ | ✓ |
| [`unserialize()`](./misc/unserialize.md) | `(string $data, mixed $options = []): mixed` | `mixed` | ✓ | — |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` | ✓ | ✓ |
| [`user_error()`](./misc/user_error.md) | `(string $message, int $error_level = 1024): bool` | `bool` | ✓ | ✓ |
| [`var_dump()`](./misc/var_dump.md) | `(mixed $value, ...$values): void` | `void` | ✓ | ✓ |
| [`zend_version()`](./misc/zend_version.md) | `(): string` | `string` | ✓ | ✓ |
