---
title: "timezone_offset_get()"
description: "Returns a timezone's UTC offset in seconds at the given date."
sidebar:
  order: 251
---

## timezone_offset_get()

```php
function timezone_offset_get(mixed $object, mixed $datetime): int
```

Returns a timezone's UTC offset in seconds at the given date.

**Parameters**:
- `$object` (`mixed`)
- `$datetime` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_offset_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_offset_get.md).
