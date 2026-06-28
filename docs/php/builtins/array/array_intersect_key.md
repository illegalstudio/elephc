---
title: "array_intersect_key()"
description: "Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys."
sidebar:
  order: 11
---

## array_intersect_key()

```php
function array_intersect_key(array $array, ...$arrays): array
```

Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_intersect_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect_key.md).

