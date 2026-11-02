---
title: "gmp_legendre()"
description: "Legendre symbol."
sidebar:
  order: 613
---

## gmp_legendre()

```php
function gmp_legendre(object|string|int $num1, object|string|int $num2): int
```

Legendre symbol.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_legendre.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_legendre.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_legendre` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_legendre.md).
