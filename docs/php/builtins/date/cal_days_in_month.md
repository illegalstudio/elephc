---
title: "cal_days_in_month()"
description: "Returns the number of days in a month of the given calendar and year."
sidebar:
  order: 186
---

## cal_days_in_month()

```php
function cal_days_in_month(int $calendar, int $month, int $year): int
```

Returns the number of days in a month of the given calendar and year.

**Parameters**:
- `$calendar` (`int`)
- `$month` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_days_in_month` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_days_in_month.md).
