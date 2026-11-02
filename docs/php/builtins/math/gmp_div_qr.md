---
title: "gmp_div_qr()"
description: "Divides two GMP numbers and returns quotient and remainder."
sidebar:
  order: 598
---

## gmp_div_qr()

```php
function gmp_div_qr(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): array
```

Divides two GMP numbers and returns quotient and remainder.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)
- `$rounding_mode` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_qr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_qr.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_div_qr` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div_qr.md).
