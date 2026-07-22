---
title: "cosh()"
description: "Returns the hyperbolic cosine of a number."
sidebar:
  order: 262
---

## cosh()

```php
function cosh(float $num): float
```

Returns the hyperbolic cosine of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/cosh.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/cosh.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `cosh` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/cosh.md).
