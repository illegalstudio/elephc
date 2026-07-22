---
title: "hypot()"
description: "Calculates the length of the hypotenuse of a right-angle triangle."
sidebar:
  order: 268
---

## hypot()

```php
function hypot(float $x, float $y): float
```

Calculates the length of the hypotenuse of a right-angle triangle.

**Parameters**:
- `$x` (`float`)
- `$y` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/hypot.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/hypot.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hypot` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/hypot.md).
