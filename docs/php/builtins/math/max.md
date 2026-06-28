---
title: "max()"
description: "Lowers numeric `min()` and `max()` over concrete integer-like or float operands."
sidebar:
  order: 240
---

## max()

```php
function max(mixed $value, ...$values): float
```

Lowers numeric `min()` and `max()` over concrete integer-like or float operands.

**Parameters**:
- `$value` (`mixed`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `float`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `max` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/max.md).

