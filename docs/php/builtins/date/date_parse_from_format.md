---
title: "date_parse_from_format()"
description: "Parses a date/time string against a format into components, warnings, and errors."
sidebar:
  order: 209
---

## date_parse_from_format()

```php
function date_parse_from_format(string $format, string $datetime): array
```

Parses a date/time string against a format into components, warnings, and errors.

**Parameters**:
- `$format` (`string`)
- `$datetime` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_parse_from_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_parse_from_format.md).
