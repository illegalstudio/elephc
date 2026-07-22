---
title: "log2()"
description: "Returns the base-2 logarithm of a number."
sidebar:
  order: 275
---

## log2()

```php
function log2(float $num): float
```

Returns the base-2 logarithm of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/log2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/log2.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `log2` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/log2.md).
