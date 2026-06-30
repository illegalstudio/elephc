---
title: "array_uintersect()"
description: "Lowers `array_uintersect()`: keeps first-array elements equal (per comparator) to some second-array element."
sidebar:
  order: 41
---

## array_uintersect()

```php
function array_uintersect(array $array1, array $array2, callable $callback): mixed
```

Lowers `array_uintersect()`: keeps first-array elements equal (per comparator) to some second-array element.

**Parameters**:
- `$array1` (`array`)
- `$array2` (`array`)
- `$callback` (`callable`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_uintersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_uintersect.md).

