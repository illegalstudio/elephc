---
title: "jewishtojd()"
description: "Converts a Jewish date into a Julian Day count."
sidebar:
  order: 237
---

## jewishtojd()

```php
function jewishtojd(int $month, int $day, int $year): int
```

Converts a Jewish date into a Julian Day count.

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

For how `jewishtojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jewishtojd.md).
