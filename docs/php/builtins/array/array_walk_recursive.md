---
title: "array_walk_recursive()"
description: "Lowers `array_walk_recursive()`: invokes the callback on each scalar leaf of a (possibly nested)"
sidebar:
  order: 46
---

## array_walk_recursive()

```php
function array_walk_recursive(array $array, callable $callback, mixed $value): void
```

Lowers `array_walk_recursive()`: invokes the callback on each scalar leaf of a (possibly nested)

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)
- `$value` (`mixed`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_walk_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk_recursive.md).

