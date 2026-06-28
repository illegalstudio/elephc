---
title: "array_unshift()"
description: "Lowers `array_unshift()` by ensuring uniqueness, prepending one scalar value, and returning count."
sidebar:
  order: 29
---

## array_unshift()

```php
function array_unshift(array $array, ...$values): int
```

Lowers `array_unshift()` by ensuring uniqueness, prepending one scalar value, and returning count.

**Parameters**:
- `$array` (`array`), passed by reference
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_unshift` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_unshift.md).

