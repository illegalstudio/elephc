---
title: "array_intersect_assoc()"
description: "Lowers `array_intersect_assoc()` via the shared associative diff/intersect helper (mode 1 = intersect)."
sidebar:
  order: 15
---

## array_intersect_assoc()

```php
function array_intersect_assoc(array $array, ...$arrays): mixed
```

Lowers `array_intersect_assoc()` via the shared associative diff/intersect helper (mode 1 = intersect).

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_intersect_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect_assoc.md).

