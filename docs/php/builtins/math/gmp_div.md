---
title: "gmp_div()"
description: "Alias of gmp_div_q()."
sidebar:
  order: 596
---

## gmp_div()

```php
function gmp_div(object|string|int $num1, object|string|int $num2, int $rounding_mode = 0): object
```

Alias of gmp_div_q().

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)
- `$rounding_mode` (`int`), default `0`, optional

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_div.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_div` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_div.md).
