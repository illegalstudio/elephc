---
title: "Misc builtins"
description: "Builtins in the Misc category."
sidebar:
  order: 118
---

## Misc builtins

| Function | Signature | Returns |
|---|---|---|
| [`buffer_new()`](./misc/buffer_new.md) | `(int $length): mixed` | `mixed` |
| [`call_user_func()`](./misc/call_user_func.md) | `(callable $callback, ...$args): mixed` | `mixed` |
| [`call_user_func_array()`](./misc/call_user_func_array.md) | `(callable $callback, array $args): mixed` | `mixed` |
| [`define()`](./misc/define.md) | `(string $constant_name, mixed $value, bool $case_insensitive): bool` | `bool` |
| [`defined()`](./misc/defined.md) | `(string $constant_name): bool` | `bool` |
| [`empty()`](./misc/empty.md) | `(mixed $value): bool` | `bool` |
| [`header()`](./misc/header.md) | `(mixed $header, mixed $replace, mixed $response_code): void` | `void` |
| [`http_response_code()`](./misc/http_response_code.md) | `(mixed $response_code): int` | `int` |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): int` | `int` |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode): string` | `string` |
| [`phpversion()`](./misc/phpversion.md) | `(string $extension = null): string` | `string` |
| [`print_r()`](./misc/print_r.md) | `(...$values): void` | `void` |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` |
| [`var_dump()`](./misc/var_dump.md) | `(...$values): void` | `void` |
