---
title: "usort()"
description: "Lowers `usort()` for indexed integer arrays with a static user comparator."
sidebar:
  order: 46
---

## usort()

```php
function usort(array $array, callable $callback): bool
```

Lowers `usort()` for indexed integer arrays with a static user comparator.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `usort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/usort.md).

