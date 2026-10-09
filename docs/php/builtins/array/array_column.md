---
title: "array_column()"
description: "Returns the values from a single column of an array of arrays."
sidebar:
  order: 4
---

## array_column()

```php
function array_column(array $array, int|string|null $column_key, int|string|null $index_key = null): array
```

Returns the values from a single column of an array of arrays.

**Parameters**:
- `$array` (`array`)
- `$column_key` (`int|string|null`)
- `$index_key` (`int|string|null`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_column.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_column.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_column` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_column.md).
