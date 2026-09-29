---
title: "easter_date()"
description: "Returns the Unix timestamp of midnight on Easter Sunday of a year."
sidebar:
  order: 219
---

## easter_date()

```php
function easter_date(?int $year = null, int $mode = CAL_EASTER_DEFAULT): int
```

Returns the Unix timestamp of midnight on Easter Sunday of a year.

**Parameters**:
- `$year` (`?int`), default `null`, optional
- `$mode` (`int`), default `CAL_EASTER_DEFAULT`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `easter_date` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/easter_date.md).
