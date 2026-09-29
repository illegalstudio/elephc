---
title: "date_format()"
description: "Formats a date according to a format string."
sidebar:
  order: 201
---

## date_format()

```php
function date_format(mixed $object, string $format): string
```

Formats a date according to a format string.

**Parameters**:
- `$object` (`mixed`)
- `$format` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_format.md).
