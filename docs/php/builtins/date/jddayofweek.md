---
title: "jddayofweek()"
description: "Returns the day of the week for a Julian Day count, as a number or a name."
---

## jddayofweek()

```php
function jddayofweek(int $julian_day, int $mode = CAL_DOW_DAYNO): mixed
```

Returns the day of the week for a Julian Day count, as a number or a name.

**Parameters**:
- `$julian_day` (`int`)
- `$mode` (`int`), default `CAL_DOW_DAYNO`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jddayofweek` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jddayofweek.md).
