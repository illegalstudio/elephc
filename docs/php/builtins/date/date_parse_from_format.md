---
title: "date_parse_from_format()"
description: "Parses a date/time string against a format into components, warnings, and errors."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_parse_from_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_parse_from_format.md).
