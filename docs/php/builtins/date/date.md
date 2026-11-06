---
title: "date()"
description: "Formats a local time/date."
---

## date()

```php
function date(string $format, ?int $timestamp = null): string
```

Formats a local time/date.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`?int`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date.md).
