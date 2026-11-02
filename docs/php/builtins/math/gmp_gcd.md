---
title: "gmp_gcd()"
description: "Greatest common divisor."
sidebar:
  order: 603
---

## gmp_gcd()

```php
function gmp_gcd(object|string|int $num1, object|string|int $num2): object
```

Greatest common divisor.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_gcd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_gcd.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_gcd` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_gcd.md).
