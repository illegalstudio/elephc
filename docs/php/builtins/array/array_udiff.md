---
title: "array_udiff()"
description: "Lowers `array_udiff()`: keeps first-array elements not equal (per comparator) to any second-array element."
sidebar:
  order: 40
---

## array_udiff()

```php
function array_udiff(array $array1, array $array2, callable $callback): mixed
```

Lowers `array_udiff()`: keeps first-array elements not equal (per comparator) to any second-array element.

**Parameters**:
- `$array1` (`array`)
- `$array2` (`array`)
- `$callback` (`callable`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_udiff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_udiff.md).

