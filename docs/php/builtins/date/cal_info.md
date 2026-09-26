---
title: "cal_info()"
description: "Returns a calendar's month names, abbreviations, and day count."
sidebar:
  order: 188
---

## cal_info()

```php
function cal_info(int $calendar = -1): array
```

Returns a calendar's month names, abbreviations, and day count.

**Parameters**:
- `$calendar` (`int`), default `-1`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_info.md).
