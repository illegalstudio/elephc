---
title: "min()"
description: "Lowers numeric `min()` and `max()` over concrete integer-like or float operands."
sidebar:
  order: 241
---

## min()

```php
function min(mixed $value, ...$values): float
```

Lowers numeric `min()` and `max()` over concrete integer-like or float operands.

**Parameters**:
- `$value` (`mixed`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `float`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `min` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/min.md).

