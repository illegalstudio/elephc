---
title: "round()"
description: "Rounds a float."
sidebar:
  order: 284
---

## round()

```php
function round(float $num, int $precision = 0): float
```

Rounds a float.

**Parameters**:
- `$num` (`float`)
- `$precision` (`int`), default `0`, optional

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/round.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/round.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `round` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/round.md).
