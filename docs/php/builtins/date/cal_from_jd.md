---
title: "cal_from_jd()"
description: "Converts a Julian Day count into a date array for the given calendar."
---

## cal_from_jd()

```php
function cal_from_jd(int $julian_day, int $calendar): array
```

Converts a Julian Day count into a date array for the given calendar.

**Parameters**:
- `$julian_day` (`int`)
- `$calendar` (`int`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_from_jd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_from_jd.md).
