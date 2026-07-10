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
| [`define()`](./misc/define.md) | `(string $constant_name, mixed $value): bool` | `bool` |
| [`defined()`](./misc/defined.md) | `(string $constant_name): bool` | `bool` |
| [`empty()`](./misc/empty.md) | `(mixed $value): bool` | `bool` |
| [`header()`](./misc/header.md) | `(string $header, bool $replace = true, int $response_code = 0): void` | `void` |
| [`http_response_code()`](./misc/http_response_code.md) | `(int $response_code = 0): int` | `int` |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): bool` | `bool` |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode = 'a'): string` | `string` |
| [`phpversion()`](./misc/phpversion.md) | `(): string` | `string` |
| [`print_r()`](./misc/print_r.md) | `(mixed $value, bool $return = false): mixed` | `mixed` |
| [`serialize()`](./misc/serialize.md) | `(mixed $value): string` | `string` |
| [`unserialize()`](./misc/unserialize.md) | `(string $data, mixed $options = []): mixed` | `mixed` |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` |
| [`var_dump()`](./misc/var_dump.md) | `(mixed $value, ...$values): void` | `void` |
