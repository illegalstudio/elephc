---
title: "ptr_read_f64()"
description: "Reads one IEEE 754 double-precision value through a raw pointer and returns it as a float."
sidebar:
  order: 740
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_f64.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_f64.rs)).
- **Strict PHP mode**: hidden - this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_read_f64` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_f64.md).
