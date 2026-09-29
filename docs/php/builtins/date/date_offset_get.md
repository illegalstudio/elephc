---
title: "date_offset_get()"
description: "Returns a date's UTC offset in seconds."
sidebar:
  order: 207
---

## date_offset_get()

```php
function date_offset_get(mixed $object): int
```

Returns a date's UTC offset in seconds.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_offset_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_offset_get.md).
