---
title: "acos()"
description: "Returns the arccosine of a number in radians."
sidebar:
  order: 255
---

## acos()

```php
function acos(float $num): float
```

Returns the arccosine of a number in radians.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/acos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/acos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `acos` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/acos.md).
