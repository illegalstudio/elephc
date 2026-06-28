---
title: "array_map()"
description: "Lowers `array_map()` through the callback runtime helper matching the callback result type."
sidebar:
  order: 14
---

## array_map()

```php
function array_map(callable $callback, array $array, ...$arrays): array
```

Lowers `array_map()` through the callback runtime helper matching the callback result type.

**Parameters**:
- `$callback` (`callable`)
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_map` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_map.md).

