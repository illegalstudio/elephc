---
title: "gmp_div_q()"
description: "Divides two GMP numbers."
sidebar:
  order: 597
---

## gmp_div_q()

```php
function gmp_div_q(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object
```

Divides two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)
- `$rounding_mode` (`int`), default `0`, optional

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_q.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div_q.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_div_q` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div_q.md).
