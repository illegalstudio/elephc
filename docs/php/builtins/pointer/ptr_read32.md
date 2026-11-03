---
title: "ptr_read32()"
description: "Reads one unsigned 32-bit word through a raw pointer and returns it as an integer."
---

## ptr_read32()

```php
function ptr_read32(pointer $pointer): int
```

Reads one unsigned 32-bit word through a raw pointer and returns it as an integer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_read32` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read32.md).
