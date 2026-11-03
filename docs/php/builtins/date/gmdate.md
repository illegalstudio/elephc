---
title: "gmdate()"
description: "Formats a GMT/UTC date and time."
---

## gmdate()

```php
function gmdate(string $format, ?int $timestamp = null): string
```

Formats a GMT/UTC date and time.

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

For how `gmdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmdate.md).
