---
title: "checkdate()"
description: "Validates a Gregorian date."
---

## checkdate()

```php
function checkdate(int $month, int $day, int $year): bool
```

Validates a Gregorian date.

**Parameters**:
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `checkdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/checkdate.md).
