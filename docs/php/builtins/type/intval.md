---
title: "intval()"
description: "Returns the integer value of a variable, optionally using a given base."
---

## intval()

```php
function intval(mixed $value, int $base = 10): int
```

Returns the integer value of a variable, optionally using a given base.

**Parameters**:
- `$value` (`mixed`)
- `$base` (`int`), default `10`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/intval.md).
