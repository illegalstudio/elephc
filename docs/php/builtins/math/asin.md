---
title: "asin()"
description: "Returns the arcsine of a number in radians."
sidebar:
  order: 254
---

## asin()

```php
function asin(float $num): float
```

Returns the arcsine of a number in radians.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/asin.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/asin.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `asin` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/asin.md).

