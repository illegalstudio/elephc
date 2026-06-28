---
title: "Pointer builtins"
description: "Builtins in the Pointer category."
sidebar:
  order: 116
---

## Pointer builtins

| Function | Signature | Returns |
|---|---|---|
| [`ptr()`](./pointer/ptr.md) | `(mixed $value): mixed` | `mixed` |
| [`ptr_get()`](./pointer/ptr_get.md) | `(pointer $pointer): int` | `int` |
| [`ptr_is_null()`](./pointer/ptr_is_null.md) | `(pointer $pointer): bool` | `bool` |
| [`ptr_null()`](./pointer/ptr_null.md) | `(): mixed` | `mixed` |
| [`ptr_offset()`](./pointer/ptr_offset.md) | `(pointer $pointer, int $offset): mixed` | `mixed` |
| [`ptr_read16()`](./pointer/ptr_read16.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read32()`](./pointer/ptr_read32.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read8()`](./pointer/ptr_read8.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read_string()`](./pointer/ptr_read_string.md) | `(pointer $pointer, int $length): string` | `string` |
| [`ptr_set()`](./pointer/ptr_set.md) | `(pointer $pointer, mixed $value): void` | `void` |
| [`ptr_sizeof()`](./pointer/ptr_sizeof.md) | `(string $type): mixed` | `mixed` |
| [`ptr_write16()`](./pointer/ptr_write16.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write32()`](./pointer/ptr_write32.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write8()`](./pointer/ptr_write8.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write_string()`](./pointer/ptr_write_string.md) | `(pointer $pointer, string $string): int` | `int` |
