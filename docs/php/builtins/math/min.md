---
title: "min()"
description: "Find lowest value."
---

## min()

```php
function min(mixed $value, ...$values): mixed
```

Find lowest value.

**Parameters**:
- `$value` (`mixed`)
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `min` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/min.md).
