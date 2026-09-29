---
title: "date_timestamp_set()"
description: "Sets a DateTime from a Unix timestamp."
sidebar:
  order: 216
---

## date_timestamp_set()

```php
function date_timestamp_set(mixed $object, int $timestamp): mixed
```

Sets a DateTime from a Unix timestamp.

**Parameters**:
- `$object` (`mixed`)
- `$timestamp` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timestamp_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timestamp_set.md).
