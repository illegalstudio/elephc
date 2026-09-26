---
title: "date_date_set()"
description: "Sets a DateTime's year, month, and day."
sidebar:
  order: 197
---

## date_date_set()

```php
function date_date_set(mixed $object, int $year, int $month, int $day): mixed
```

Sets a DateTime's year, month, and day.

**Parameters**:
- `$object` (`mixed`)
- `$year` (`int`)
- `$month` (`int`)
- `$day` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_date_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_date_set.md).
