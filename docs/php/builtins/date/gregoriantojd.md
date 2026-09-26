---
title: "gregoriantojd()"
description: "Converts a Gregorian date into a Julian Day count."
sidebar:
  order: 227
---

## gregoriantojd()

```php
function gregoriantojd(int $month, int $day, int $year): int
```

Converts a Gregorian date into a Julian Day count.

**Parameters**:
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gregoriantojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gregoriantojd.md).
