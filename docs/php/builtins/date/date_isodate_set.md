---
title: "date_isodate_set()"
description: "Sets a DateTime from an ISO year, week, and day of week."
sidebar:
  order: 205
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

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_isodate_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_isodate_set.md).
