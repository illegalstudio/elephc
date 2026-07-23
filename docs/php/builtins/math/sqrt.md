---
title: "sqrt()"
description: "Returns the square root of a number."
sidebar:
  order: 288
---

## sqrt()

```php
function sqrt(float $num): float
```

Returns the square root of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/sqrt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/sqrt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sqrt` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/sqrt.md).
