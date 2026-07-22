---
title: "tan()"
description: "Returns the tangent of a number (radians)."
sidebar:
  order: 288
---

## tan()

```php
function tan(float $num): float
```

Returns the tangent of a number (radians).

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/tan.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/tan.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `tan` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/tan.md).
