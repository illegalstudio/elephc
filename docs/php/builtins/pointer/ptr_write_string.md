---
title: "ptr_write_string()"
description: "Copies PHP string bytes into raw memory at the given pointer."
---

## ptr_write_string()

```php
function ptr_write_string(pointer $pointer, string $string): int
```

Copies PHP string bytes into raw memory at the given pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$string` (`string`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_write_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_string.md).
