---
title: "array_column()"
description: "Returns the values from a single column of an array of arrays."
sidebar:
  order: 4
---

## array_column()

```php
function array_column(array $array, string $column_key): array
```

Returns the values from a single column of an array of arrays.

**Parameters**:
- `$array` (`array`)
- `$column_key` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_column.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_column.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_column` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_column.md).
