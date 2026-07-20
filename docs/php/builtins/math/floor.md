---
title: "floor()"
description: "Rounds a number down to the nearest integer."
sidebar:
  order: 264
---

## floor()

```php
function floor(float $num): float
```

Rounds a number down to the nearest integer.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/floor.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/floor.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `floor` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/floor.md).

