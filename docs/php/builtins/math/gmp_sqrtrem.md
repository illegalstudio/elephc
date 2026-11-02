---
title: "gmp_sqrtrem()"
description: "Integer square root and remainder."
sidebar:
  order: 635
---

## gmp_sqrtrem()

```php
function gmp_sqrtrem(object|string|int $num): array
```

Integer square root and remainder.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sqrtrem.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sqrtrem.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_sqrtrem` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_sqrtrem.md).
