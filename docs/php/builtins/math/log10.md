---
title: "log10()"
description: "Returns the base-10 logarithm of a number."
sidebar:
  order: 259
---

## log10()

```php
function log10(float $num): float
```

Returns the base-10 logarithm of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/log10.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/log10.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `log10` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/log10.md).

