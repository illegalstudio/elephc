---
title: "uksort()"
description: "Lowers `uksort()` through the legacy user-sort helper for static comparators."
sidebar:
  order: 45
---

## uksort()

```php
function uksort(array $array, callable $callback): bool
```

Lowers `uksort()` through the legacy user-sort helper for static comparators.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `uksort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/uksort.md).

