---
title: "date_timestamp_get()"
description: "Returns a date's Unix timestamp."
sidebar:
  order: 215
---

## date_timestamp_get()

```php
function date_timestamp_get(mixed $object): int
```

Returns a date's Unix timestamp.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timestamp_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timestamp_get.md).
