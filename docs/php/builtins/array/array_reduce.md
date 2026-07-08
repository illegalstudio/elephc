---
title: "array_reduce()"
description: "Iteratively reduces an array to a single value using a callback function."
sidebar:
  order: 31
---

## array_reduce()

```php
function array_reduce(array $array, callable $callback, mixed $initial = null): int
```

Iteratively reduces an array to a single value using a callback function.

**Parameters**:
- `$array` (`array`)
- `$callback` (`callable`)
- `$initial` (`mixed`), default `null`, optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_reduce` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_reduce.md).

