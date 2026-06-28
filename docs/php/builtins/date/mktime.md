---
title: "mktime()"
description: "Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper."
sidebar:
  order: 78
---

## mktime()

```php
function mktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper.

**Parameters**:
- `$hour` (`int`)
- `$minute` (`int`)
- `$second` (`int`)
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/mktime.md).

