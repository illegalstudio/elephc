---
title: "mktime()"
description: "Returns the Unix timestamp for a date."
sidebar:
  order: 95
---

## mktime()

```php
function mktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

Returns the Unix timestamp for a date.

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

