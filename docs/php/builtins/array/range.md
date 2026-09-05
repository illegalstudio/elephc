---
title: "range()"
description: "Create an array containing a range of elements."
sidebar:
  order: 63
---

## range()

```php
function range(mixed $start, mixed $end, int $step = 1): array
```

Create an array containing a range of elements.

**Parameters**:
- `$start` (`mixed`)
- `$end` (`mixed`)
- `$step` (`int`), default `1`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/range.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/range.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `range` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/range.md).
