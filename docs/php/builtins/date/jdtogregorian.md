---
title: "jdtogregorian()"
description: "Converts a Julian Day count into a Gregorian date string."
sidebar:
  order: 233
---

## jdtogregorian()

```php
function jdtogregorian(int $julian_day): string
```

Converts a Julian Day count into a Gregorian date string.

**Parameters**:
- `$julian_day` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtogregorian` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtogregorian.md).
