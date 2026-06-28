---
title: "array_column()"
description: "Lowers `array_column()` by dispatching to the helper matching row value ownership."
sidebar:
  order: 2
---

## array_column()

```php
function array_column(array $array, string $column_key, string $index_key): array
```

Lowers `array_column()` by dispatching to the helper matching row value ownership.

**Parameters**:
- `$array` (`array`)
- `$column_key` (`string`)
- `$index_key` (`string`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_column` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_column.md).

