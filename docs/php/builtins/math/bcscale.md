---
title: "bcscale()"
description: "Gets or sets the process-wide default BCMath scale."
sidebar:
  order: 279
---

## bcscale()

```php
function bcscale(int $scale = null): int
```

Gets or sets the process-wide default BCMath scale.

**Parameters**:
- `$scale` (`int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcscale.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcscale.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcscale` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcscale.md).
