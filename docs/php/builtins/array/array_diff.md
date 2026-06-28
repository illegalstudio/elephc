---
title: "array_diff()"
description: "Lowers `array_diff()` for two compatible indexed arrays with pointer-sized payload slots."
sidebar:
  order: 4
---

## array_diff()

```php
function array_diff(array $array, ...$arrays): array
```

Lowers `array_diff()` for two compatible indexed arrays with pointer-sized payload slots.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff.md).

