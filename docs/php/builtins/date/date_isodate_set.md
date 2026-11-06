---
title: "date_isodate_set()"
description: "Sets a DateTime from an ISO year, week, and day of week."
---

## date_isodate_set()

```php
function date_isodate_set(mixed $object, int $year, int $week, int $dayOfWeek = 1): mixed
```

Sets a DateTime from an ISO year, week, and day of week.

**Parameters**:
- `$object` (`mixed`)
- `$year` (`int`)
- `$week` (`int`)
- `$dayOfWeek` (`int`), default `1`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_isodate_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_isodate_set.md).
