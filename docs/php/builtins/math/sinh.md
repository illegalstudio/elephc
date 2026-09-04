---
title: "sinh()"
description: "Returns the hyperbolic sine of a number."
sidebar:
  order: 314
---

## sinh()

```php
function sinh(float $num): float
```

Returns the hyperbolic sine of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/sinh.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/sinh.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sinh` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/sinh.md).
