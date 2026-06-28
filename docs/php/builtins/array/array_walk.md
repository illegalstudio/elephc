---
title: "array_walk()"
description: "Lowers `array_walk()` through the callback-driven runtime helper."
sidebar:
  order: 31
---

## array_walk()

```php
function array_walk(array $array, callable $callback, mixed $arg): void
```

Lowers `array_walk()` through the callback-driven runtime helper.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)
- `$arg` (`mixed`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_walk` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk.md).

