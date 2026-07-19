---
title: "exp()"
description: "Returns e raised to the power of a number."
sidebar:
  order: 249
---

## exp()

```php
function exp(float $num): float
```

Returns e raised to the power of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/exp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/exp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `exp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/exp.md).

