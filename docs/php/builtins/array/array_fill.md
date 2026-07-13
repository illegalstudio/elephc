---
title: "array_fill()"
description: "Fill an array with values."
sidebar:
  order: 9
---

## array_fill()

```php
function array_fill(int $start_index, int $count, mixed $value): array
```

Fill an array with values.

**Parameters**:
- `$start_index` (`int`)
- `$count` (`int`)
- `$value` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_fill.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_fill.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_fill` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_fill.md).

