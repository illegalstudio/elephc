---
title: "jdmonthname()"
description: "Returns the month name for a Julian Day count in the requested calendar."
sidebar:
  order: 231
---

## jdmonthname()

```php
function jdmonthname(int $julian_day, int $mode): string
```

Returns the month name for a Julian Day count in the requested calendar.

**Parameters**:
- `$julian_day` (`int`)
- `$mode` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdmonthname` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdmonthname.md).
