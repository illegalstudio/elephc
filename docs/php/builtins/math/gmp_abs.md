---
title: "gmp_abs()"
description: "Absolute value of a GMP number."
sidebar:
  order: 589
---

## gmp_abs()

```php
function gmp_abs(object|string|int $num): object
```

Absolute value of a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_abs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_abs.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_abs` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_abs.md).
