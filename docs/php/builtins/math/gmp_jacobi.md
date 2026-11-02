---
title: "gmp_jacobi()"
description: "Jacobi symbol."
sidebar:
  order: 610
---

## gmp_jacobi()

```php
function gmp_jacobi(object|string|int $num1, object|string|int $num2): int
```

Jacobi symbol.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_jacobi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_jacobi.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_jacobi` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_jacobi.md).
