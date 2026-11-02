---
title: "gmp_kronecker()"
description: "Kronecker symbol."
sidebar:
  order: 611
---

## gmp_kronecker()

```php
function gmp_kronecker(object|string|int $num1, object|string|int $num2): int
```

Kronecker symbol.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_kronecker.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_kronecker.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_kronecker` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_kronecker.md).
