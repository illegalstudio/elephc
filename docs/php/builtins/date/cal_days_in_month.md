---
title: "cal_days_in_month()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 183
---

## cal_days_in_month()

```php
function cal_days_in_month(int $calendar, int $month, int $year): int
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$calendar` (`int`)
- `$month` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_days_in_month` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_days_in_month.md).
