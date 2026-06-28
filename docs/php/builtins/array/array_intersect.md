---
title: "array_intersect()"
description: "Lowers `array_intersect()` for two compatible indexed arrays with pointer-sized payload slots."
sidebar:
  order: 10
---

## array_intersect()

```php
function array_intersect(array $array, ...$arrays): array
```

Lowers `array_intersect()` for two compatible indexed arrays with pointer-sized payload slots.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_intersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect.md).

