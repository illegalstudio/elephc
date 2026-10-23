---
title: "ptr_write_f32()"
description: "Writes one IEEE 754 single-precision value through a raw pointer, rounding the float to 32 bits."
sidebar:
  order: 747
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write_f32.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write_f32.rs)).
- **Strict PHP mode**: hidden - this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_write_f32` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_f32.md).
