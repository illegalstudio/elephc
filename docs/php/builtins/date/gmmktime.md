---
title: "gmmktime()"
description: "Returns the Unix timestamp for a GMT date."
---

## gmmktime()

```php
function gmmktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

Returns the Unix timestamp for a GMT date.

**Parameters**:
- `$hour` (`int`)
- `$minute` (`int`)
- `$second` (`int`)
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

For how `gmmktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmmktime.md).
