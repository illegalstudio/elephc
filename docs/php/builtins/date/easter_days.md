---
title: "easter_days()"
description: "Returns the number of days from March 21 to Easter Sunday of a year."
---

## easter_days()

```php
function easter_days(?int $year = null, int $mode = CAL_EASTER_DEFAULT): int
```

Returns the number of days from March 21 to Easter Sunday of a year.

**Parameters**:
- `$year` (`?int`), default `null`, optional
- `$mode` (`int`), default `CAL_EASTER_DEFAULT`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `easter_days` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/easter_days.md).
