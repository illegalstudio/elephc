---
title: "checkdate()"
description: "Lowers `checkdate(month, day, year)` through the shared Gregorian-validation runtime helper."
sidebar:
  order: 68
---

## checkdate()

```php
function checkdate(int $month, int $day, int $year): bool
```

Lowers `checkdate(month, day, year)` through the shared Gregorian-validation runtime helper.

**Parameters**:
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `checkdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/checkdate.md).

