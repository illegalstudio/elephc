---
title: "gregoriantojd()"
description: "Converts a Gregorian date into a Julian Day count."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gregoriantojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gregoriantojd.md).
