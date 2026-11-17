---
title: "ptr_read_f64()"
description: "Reads one IEEE 754 double-precision value through a raw pointer and returns it as a float."
---

## ptr_read_f64()

```php
function ptr_read_f64(pointer $pointer): float
```

Reads one IEEE 754 double-precision value through a raw pointer and returns it as a float.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `float`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_read_f64` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_f64.md).
