---
title: "pow()"
description: "Exponential expression."
sidebar:
  order: 308
---

## pow()

```php
function pow(float $num, float $exponent): float
```

Exponential expression.

**Parameters**:
- `$num` (`float`)
- `$exponent` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/pow.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/pow.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pow` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/pow.md).
