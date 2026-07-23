---
title: "sin()"
description: "Returns the sine of a number (radians)."
sidebar:
  order: 286
---

## sin()

```php
function sin(float $num): float
```

Returns the sine of a number (radians).

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/sin.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/sin.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sin` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/sin.md).
