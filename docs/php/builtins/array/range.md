---
title: "range()"
description: "Create an array containing a range of elements."
sidebar:
  order: 57
---

## range()

```php
function range(mixed $start, mixed $end): array
```

Create an array containing a range of elements.

**Parameters**:
- `$start` (`mixed`)
- `$end` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/range.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/range.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `range` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/range.md).
