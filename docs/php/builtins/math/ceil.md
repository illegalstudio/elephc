---
title: "ceil()"
description: "Rounds a number up to the nearest integer."
sidebar:
  order: 259
---

## ceil()

```php
function ceil(float $num): float
```

Rounds a number up to the nearest integer.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/ceil.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/ceil.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ceil` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/ceil.md).
