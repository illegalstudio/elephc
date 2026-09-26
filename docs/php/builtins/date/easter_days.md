---
title: "easter_days()"
description: "Returns the number of days from March 21 to Easter Sunday of a year."
sidebar:
  order: 220
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

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `easter_days` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/easter_days.md).
