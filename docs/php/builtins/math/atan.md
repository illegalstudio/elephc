---
title: "atan()"
description: "Returns the arctangent of a number in radians."
sidebar:
  order: 255
---

## atan()

```php
function atan(float $num): float
```

Returns the arctangent of a number in radians.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/atan.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/atan.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `atan` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/atan.md).

