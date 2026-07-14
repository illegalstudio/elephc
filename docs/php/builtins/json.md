---
title: "JSON builtins"
description: "Builtins in the JSON category."
sidebar:
  order: 105
---

## JSON builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`json_decode()`](./json/json_decode.md) | `(string $json, bool $associative = null, int $depth = 512, int $flags = 0): mixed` | `mixed` | ✓ | ✓ |
| [`json_encode()`](./json/json_encode.md) | `(mixed $value, int $flags = 0, int $depth = 512): string` | `string` | ✓ | ✓ |
| [`json_last_error()`](./json/json_last_error.md) | `(): int` | `int` | ✓ | ✓ |
| [`json_last_error_msg()`](./json/json_last_error_msg.md) | `(): string` | `string` | ✓ | ✓ |
| [`json_validate()`](./json/json_validate.md) | `(string $json, int $depth = 512, int $flags = 0): bool` | `bool` | ✓ | ✓ |
