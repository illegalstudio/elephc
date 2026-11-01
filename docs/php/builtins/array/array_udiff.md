---
title: "array_udiff()"
description: "Returns entries from the first of exactly two arrays whose values are absent from the second according to an integer-cast callback comparator, preserving keys."
sidebar:
  order: 49
---

## array_udiff()

```php
function array_udiff(array $array1, array $array2, callable $callback): array
```

Returns entries from the first of exactly two arrays whose values are absent from the second according to an integer-cast callback comparator, preserving keys.

**Parameters**:
- `$array1` (`array`)
- `$array2` (`array`)
- `$callback` (`callable`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_udiff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_udiff.md).
