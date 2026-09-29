---
title: "cal_to_jd()"
description: "Converts a date in the given calendar into a Julian Day count."
sidebar:
  order: 189
---

## cal_to_jd()

```php
function cal_to_jd(int $calendar, int $month, int $day, int $year): int
```

Converts a date in the given calendar into a Julian Day count.

**Parameters**:
- `$calendar` (`int`)
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_to_jd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_to_jd.md).
