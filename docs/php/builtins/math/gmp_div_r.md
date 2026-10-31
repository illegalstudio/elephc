---
title: "gmp_div_r()"
description: "Returns the remainder of a GMP division."
sidebar:
  order: 599
---

## gmp_div_r()

```php
function gmp_div_r(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object
```

Returns the remainder of a GMP division.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)
- `$rounding_mode` (`int`), default `0`, optional

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_r.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_r.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_div_r` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div_r.md).
