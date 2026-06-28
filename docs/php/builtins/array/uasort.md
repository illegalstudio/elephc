---
title: "uasort()"
description: "Lowers `uasort()` through the legacy user-sort helper for static comparators."
sidebar:
  order: 44
---

## uasort()

```php
function uasort(array $array, callable $callback): bool
```

Lowers `uasort()` through the legacy user-sort helper for static comparators.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `uasort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/uasort.md).

