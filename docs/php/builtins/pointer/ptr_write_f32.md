---
title: "ptr_write_f32()"
description: "Writes one IEEE 754 single-precision value through a raw pointer, rounding the float to 32 bits."
---

## ptr_write_f32()

```php
function ptr_write_f32(pointer $pointer, mixed $value): void
```

Writes one IEEE 754 single-precision value through a raw pointer, rounding the float to 32 bits.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`mixed`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_write_f32` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_f32.md).
