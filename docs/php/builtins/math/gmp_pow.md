---
title: "gmp_pow()"
description: "Raises a GMP number to an integer power."
sidebar:
  order: 622
---

## gmp_pow()

```php
function gmp_pow(object|string|int $num, int $exponent): object
```

Raises a GMP number to an integer power.

**Parameters**:
- `$num` (`object|string|int`)
- `$exponent` (`int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_pow.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_pow.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_pow` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_pow.md).
