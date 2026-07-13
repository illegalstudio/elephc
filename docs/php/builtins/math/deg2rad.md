---
title: "deg2rad()"
description: "Converts a degree value to radians."
sidebar:
  order: 248
---

## deg2rad()

```php
function deg2rad(float $num): float
```

Converts a degree value to radians.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/deg2rad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/deg2rad.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `deg2rad` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/deg2rad.md).

