---
title: "timezone_name_get()"
description: "Returns a timezone's identifier."
sidebar:
  order: 250
---

## timezone_name_get()

```php
function timezone_name_get(mixed $object): string
```

Returns a timezone's identifier.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_name_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_name_get.md).
