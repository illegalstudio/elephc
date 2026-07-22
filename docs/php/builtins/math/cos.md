---
title: "cos()"
description: "Returns the cosine of a number (radians)."
sidebar:
  order: 261
---

## cos()

```php
function cos(float $num): float
```

Returns the cosine of a number (radians).

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/cos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/cos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `cos` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/cos.md).
