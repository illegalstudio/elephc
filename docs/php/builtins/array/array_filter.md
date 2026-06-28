---
title: "array_filter()"
description: "Lowers `array_filter()` for static and first-class callbacks through the runtime helper."
sidebar:
  order: 8
---

## array_filter()

```php
function array_filter(array $array, callable $callback, int $mode): array
```

Lowers `array_filter()` for static and first-class callbacks through the runtime helper.

**Parameters**:
- `$array` (`array`)
- `$callback` (`callable`), optional
- `$mode` (`int`), optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_filter` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_filter.md).

