---
title: "ptr_read8()"
description: "Reads one unsigned byte through a raw pointer and returns it as an integer."
---

## ptr_read8()

```php
function ptr_read8(pointer $pointer): int
```

Reads one unsigned byte through a raw pointer and returns it as an integer.

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

For how `ptr_read8` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read8.md).
