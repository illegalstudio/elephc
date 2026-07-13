---
title: "atan2()"
description: "Returns the arc tangent of two variables."
sidebar:
  order: 243
---

## atan2()

```php
function atan2(float $y, float $x): float
```

Returns the arc tangent of two variables.

**Parameters**:
- `$y` (`float`)
- `$x` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/atan2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/atan2.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `atan2` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/atan2.md).

