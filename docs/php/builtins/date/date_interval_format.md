---
title: "date_interval_format()"
description: "Formats a DateInterval according to a format string."
sidebar:
  order: 204
---

## date_interval_format()

```php
function date_interval_format(mixed $object, string $format): string
```

Formats a DateInterval according to a format string.

**Parameters**:
- `$object` (`mixed`)
- `$format` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_interval_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_interval_format.md).
