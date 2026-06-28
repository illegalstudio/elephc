---
title: "array_reduce()"
description: "Lowers `array_reduce()` through the callback-driven runtime helper."
sidebar:
  order: 21
---

## array_reduce()

```php
function array_reduce(array $array, callable $callback, mixed $initial): int
```

Lowers `array_reduce()` through the callback-driven runtime helper.

**Parameters**:
- `$array` (`array`)
- `$callback` (`callable`)
- `$initial` (`mixed`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_reduce` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_reduce.md).

