---
title: "rad2deg()"
description: "Converts a radian value to degrees."
sidebar:
  order: 281
---

## rad2deg()

```php
function rad2deg(float $num): float
```

Converts a radian value to degrees.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/rad2deg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/rad2deg.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rad2deg` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/rad2deg.md).
