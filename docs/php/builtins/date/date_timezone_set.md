---
title: "date_timezone_set()"
description: "Sets a date's timezone, converting the wall-clock time to it."
sidebar:
  order: 218
---

## date_timezone_set()

```php
function date_timezone_set(mixed $object, mixed $timezone): mixed
```

Sets a date's timezone, converting the wall-clock time to it.

**Parameters**:
- `$object` (`mixed`)
- `$timezone` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timezone_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timezone_set.md).
