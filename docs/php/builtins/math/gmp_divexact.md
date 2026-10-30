---
title: "gmp_divexact()"
description: "Performs an exact GMP division."
sidebar:
  order: 600
---

## gmp_divexact()

```php
function gmp_divexact(object|string|int $num1, object|string|int $num2): object
```

Performs an exact GMP division.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_divexact.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_divexact.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_divexact` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_divexact.md).
