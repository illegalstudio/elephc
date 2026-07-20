---
title: "tanh()"
description: "Returns the hyperbolic tangent of a number."
sidebar:
  order: 287
---

## tanh()

```php
function tanh(float $num): float
```

Returns the hyperbolic tangent of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/tanh.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/tanh.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `tanh` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/tanh.md).

